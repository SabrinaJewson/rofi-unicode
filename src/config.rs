use ::{
    anyhow::{anyhow, Context as _},
    rofi_mode::pango,
    serde::{
        de::{self, Deserializer},
        Deserialize,
    },
    std::{
        env,
        ffi::OsString,
        fmt, fs, io,
        os::unix::ffi::{OsStrExt as _, OsStringExt as _},
        path::{Path, PathBuf},
    },
};

pub(crate) struct Config {
    pub(crate) root: Vec<Item>,
}

pub(crate) struct Item {
    pub(crate) name: String,
    pub(crate) name_attributes: Vec<pango::Attribute>,
    pub(crate) name_markup: String,
    pub(crate) content: Content,
}

pub(crate) enum Content {
    Text(String),
    Items(Vec<Item>),
}

pub(crate) fn read() -> anyhow::Result<Config> {
    let paths = Paths::from_env()?;

    let config_ron_path = paths.config_home().join("config.ron");
    let config_ron = fs::read_to_string(&*config_ron_path)
        .with_context(|| format!("failed to read file {}", config_ron_path.display()))?;
    let base = ron::from_str::<Base>(&config_ron)
        .with_context(|| format!("failed to parse file {}", config_ron_path.display()))?;

    let mut root = Vec::new();
    resolve(base.root, &mut root, &paths)?;

    Ok(Config { root })
}

fn resolve(
    unresolved: UnresolvedItems,
    resolved: &mut Vec<Item>,
    paths: &Paths,
) -> anyhow::Result<()> {
    resolved.reserve(unresolved.direct.len());
    for unresolved_item in unresolved.direct {
        let content = match unresolved_item.content {
            UnresolvedContent::Text(text) => Content::Text(text),
            UnresolvedContent::Items(inner_items) => {
                let mut resolved_inner_items = Vec::new();
                resolve(inner_items, &mut resolved_inner_items, paths)?;
                Content::Items(resolved_inner_items)
            }
        };

        let (name_attribute_list, name, _) = pango::parse_markup(&*unresolved_item.name, '\0')
            .with_context(|| {
                format!(
                    "item name {:?} contains invalid markup",
                    unresolved_item.name
                )
            })?;

        resolved.push(Item {
            name: name.into(),
            name_attributes: name_attribute_list.attributes(),
            name_markup: unresolved_item.name,
            content,
        });
    }

    for include_path in unresolved.extends {
        let include_ron = paths.read_to_string(&*include_path)?;
        let include = ron::from_str::<UnresolvedItems>(&*include_ron)
            .with_context(|| format!("failed to deserialize included file {include_path}"))?;

        resolve(include, resolved, paths)?;
    }

    Ok(())
}

#[derive(Deserialize)]
#[serde(rename = "Config", deny_unknown_fields)]
struct Base {
    root: UnresolvedItems,
}

struct UnresolvedItems {
    extends: Vec<String>,
    direct: Vec<UnresolvedItem>,
}

struct UnresolvedItem {
    name: String,
    content: UnresolvedContent,
}

impl<'de> Deserialize<'de> for UnresolvedItems {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        enum MapKey {
            Extends,
            Other(String),
        }
        impl<'de> Deserialize<'de> for MapKey {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl<'de> de::Visitor<'de> for Visitor {
                    type Value = MapKey;

                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("text to display on each row of the prompt")
                    }

                    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                        Ok(if v == "extends" {
                            MapKey::Extends
                        } else {
                            MapKey::Other(v.to_owned())
                        })
                    }

                    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                        Ok(if v == "extends" {
                            MapKey::Extends
                        } else {
                            MapKey::Other(v)
                        })
                    }
                }
                deserializer.deserialize_str(Visitor)
            }
        }

        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = UnresolvedItems;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map of key names to values")
            }

            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut extends = Vec::new();
                let mut direct = Vec::new();

                while let Some(key) = map.next_key::<MapKey>()? {
                    match key {
                        MapKey::Extends => {
                            extends.extend(map.next_value::<Vec<String>>()?);
                        }
                        MapKey::Other(name) => {
                            direct.push(UnresolvedItem {
                                name,
                                content: map.next_value::<UnresolvedContent>()?,
                            });
                        }
                    }
                }

                Ok(UnresolvedItems { extends, direct })
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

enum UnresolvedContent {
    Text(String),
    Items(UnresolvedItems),
}

impl<'de> Deserialize<'de> for UnresolvedContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = UnresolvedContent;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a UTF-8 string or map of items")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UnresolvedContent::Text(v.to_owned()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UnresolvedContent::Text(v))
            }
            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                Ok(UnresolvedContent::Items(UnresolvedItems::deserialize(
                    de::value::MapAccessDeserializer::new(map),
                )?))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

struct Paths {
    bases: Vec<PathBuf>,
}

impl Paths {
    fn from_env() -> anyhow::Result<Self> {
        let user_config = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                let mut home = PathBuf::from(env::var_os("HOME")?);
                home.push(".config");
                Some(home)
            })
            .context("$HOME environment variable is not set")?;

        let config_dirs = env::var_os("XDG_CONFIG_DIRS").unwrap_or_else(|| "/etc/xdg".into());
        let config_dirs = config_dirs
            .as_bytes()
            .split(|&byte| byte == b':')
            .map(|dir| PathBuf::from(OsString::from_vec(dir.to_owned())));

        let bases = [user_config]
            .into_iter()
            .chain(config_dirs)
            .map(|mut dir| {
                dir.push("rofi-unicode");
                dir
            })
            .collect();

        Ok(Self { bases })
    }

    fn config_home(&self) -> &Path {
        &*self.bases[0]
    }

    fn read_to_string(&self, path: impl AsRef<Path>) -> anyhow::Result<String> {
        let path = path.as_ref();
        if path.is_absolute() {
            fs::read_to_string(path)
                .with_context(|| format!("failed to read file {}", path.display()))
        } else {
            for base in &self.bases {
                match fs::read_to_string(base.join(path)) {
                    Ok(s) => return Ok(s),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(e)
                            .with_context(|| format!("failed to read file {}", path.display()))
                    }
                }
            }
            Err(anyhow!("could not resolve path {}", path.display()))
        }
    }
}
