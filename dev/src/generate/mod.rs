use ::{
    anyhow::Context as _,
    clap::Parser,
    serde::ser::{Serialize, SerializeMap, Serializer},
    std::{ffi::CStr, fs, path::Path, str},
};

mod de_ucd;
mod emoji;
mod ucd;

/// Generate all the system-wide default configuration files and put them in `./etc`.
#[derive(Parser)]
pub struct Args {
    /// URL or filesystem path to the Unicode website.
    #[clap(long, default_value = "https://www.unicode.org/", env)]
    unicode: String,
}

pub(crate) fn generate(Args { mut unicode }: Args) -> anyhow::Result<()> {
    if !unicode.ends_with('/') {
        unicode.push('/');
    }

    let opts = Opts {
        agent: ureq::agent(),
        unicode,
        out_dir: "etc".as_ref(),
    };

    ucd::generate(&opts)?;
    emoji::generate(&opts)?;

    Ok(())
}

struct Opts<'a> {
    agent: ureq::Agent,
    unicode: String,
    out_dir: &'a Path,
}

impl Opts<'_> {
    fn load_text_unicode(&self, path: &str) -> anyhow::Result<String> {
        let place = format!("{}{path}", self.unicode);

        if place.starts_with("http://") || place.starts_with("https://") {
            self.agent
                .get(&*place)
                .call()
                .map_err(anyhow::Error::new)
                .and_then(|res| Ok(res.into_string()?))
                .with_context(|| format!("failed to download file <{place}>"))
        } else {
            fs::read_to_string(&*place).with_context(|| format!("failed to read in file {place}"))
        }
    }

    fn write_ron(&self, relative_path: &str, items: Items) -> anyhow::Result<()> {
        let config = ron::ser::PrettyConfig::new().indentor("\t".to_owned());
        let ron = ron::ser::to_string_pretty(&items, config).unwrap();

        let path = self.out_dir.join(relative_path);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        fs::write(&*path, ron).with_context(|| format!("failed to write to {}", path.display()))?;

        println!("Successfully wrote to {relative_path}");

        Ok(())
    }
}

struct Items {
    extends: Vec<String>,
    direct: Vec<Item>,
}

impl Items {
    fn from_direct(direct: Vec<Item>) -> Self {
        Self {
            extends: Vec::new(),
            direct,
        }
    }
}

struct Item {
    name: String,
    content: Content,
}

enum Content {
    Text(String),
    Items(Items),
}

impl Serialize for Items {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("extends", &self.extends)?;
        for item in &self.direct {
            map.serialize_entry(&item.name, &item.content)?;
        }
        map.end()
    }
}

impl Serialize for Content {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Text(s) => s.serialize(serializer),
            Self::Items(items) => items.serialize(serializer),
        }
    }
}

fn with_glib_markup_escaped<O>(s: &str, f: impl FnOnce(&str) -> O) -> O {
    let escaped = unsafe { glib_sys::g_markup_escape_text(s.as_ptr().cast(), s.len() as isize) };
    let escaped_str = unsafe { CStr::from_ptr(escaped) }.to_str().unwrap();
    let _guard = defer(|| unsafe { glib_sys::g_free(escaped.cast()) });
    f(escaped_str)
}

fn defer<F: FnOnce()>(f: F) -> Defer<F> {
    Defer { function: Some(f) }
}
struct Defer<F: FnOnce()> {
    function: Option<F>,
}
impl<F: FnOnce()> Drop for Defer<F> {
    fn drop(&mut self) {
        self.function.take().unwrap()();
    }
}
