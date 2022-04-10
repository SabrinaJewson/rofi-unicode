use ::{
    anyhow::{ensure, Context as _},
    serde::{
        de::{self, Deserializer, IntoDeserializer},
        Deserialize,
    },
    std::{
        ffi::CStr,
        fmt::{self, Display, Formatter, Write as _},
        fs,
        path::Path,
        str,
    },
};

mod de_ucd;

pub(crate) fn generate(mut ucd: String, out_dir: &Path) -> anyhow::Result<()> {
    if !ucd.ends_with('/') {
        ucd.push('/');
    }

    let agent = ureq::agent();

    const UNICODE_DATA: &str = "UnicodeData.txt";
    const NAME_ALIASES: &str = "NameAliases.txt";

    let unicode_data = load_text(&agent, &*format!("{ucd}{UNICODE_DATA}"))?;
    let mut unicode_data = de_ucd::lines::<UnicodeDataLine<'_>>(&unicode_data)
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse {UNICODE_DATA}"))?;

    let name_aliases = load_text(&agent, &*format!("{ucd}{NAME_ALIASES}"))?;
    let mut name_aliases = de_ucd::lines::<NameAlias<'_>>(&name_aliases)
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse {NAME_ALIASES}"))?;

    // They're probably already sorted but we should just make sure
    unicode_data.sort_unstable_by_key(|line| line.code_point);
    name_aliases.sort_unstable_by_key(|line| line.code_point);

    let data = UnicodeData {
        unicode_data,
        name_aliases,
    };

    generate_codepoints(&data, &out_dir.join("codepoints.ron"))?;

    Ok(())
}

/// All the Unicode data.
/// Not to be confused with `UnicodeData.txt`,
/// whose data is contained in a specific field of this type.
/// Blame Unicode for the poor naming, not me.
struct UnicodeData<'a> {
    unicode_data: Vec<UnicodeDataLine<'a>>,
    name_aliases: Vec<NameAlias<'a>>,
}

fn generate_codepoints(data: &UnicodeData<'_>, output: &Path) -> anyhow::Result<()> {
    let mut ron = "{\n".to_owned();

    let mut name_aliases = data.name_aliases.iter().fuse().peekable();
    for &UnicodeDataLine {
        code_point, name, ..
    } in &data.unicode_data
    {
        if let Some(next_alias) = name_aliases.peek() {
            ensure!(
                next_alias.code_point >= code_point,
                "NameAlias.txt contains code point U+{code_point} not in UnicodeData.txt",
            );
        }

        // Name with Unicode corrections applied, if there are any
        let mut corrected_name = name;
        let mut alternate_names = String::new();
        const ALTERNATE_NAME_SEPARATOR: &str = " | ";

        while let Some(alias) = name_aliases.next_if(|alias| alias.code_point == code_point) {
            match alias.ty {
                AliasType::Correction | AliasType::Control | AliasType::Figment => {
                    corrected_name = alias.value;
                }
                AliasType::Alternate | AliasType::Abbreviation => {
                    if !alternate_names.is_empty() {
                        alternate_names.push_str(ALTERNATE_NAME_SEPARATOR);
                    }
                    alternate_names.push_str(alias.value);
                }
            }
        }

        if corrected_name.starts_with('<') {
            continue;
        }

        let printable = code_point.as_printable().unwrap_or(' ');

        let displayed_unescaped = format!("U+{code_point}\t{printable}\t{corrected_name}");

        let mut displayed = with_glib_markup_escaped(&*displayed_unescaped, |s| s.to_owned());

        if !alternate_names.is_empty() {
            with_glib_markup_escaped(&*alternate_names, |alternate_names| {
                write!(displayed, " (<small>{alternate_names}</small>)").unwrap();
            });
        }

        writeln!(
            ron,
            "\t\"{displayed}\": \"\\u{{{data}}}\",",
            displayed = displayed.escape_default(),
            data = code_point,
        )
        .unwrap();
    }

    ron.push_str("}\n");

    write_file(output, ron)?;

    println!("Successfully wrote to {}", output.display());

    Ok(())
}

/// A line of `UnicodeData.txt`.
///
/// See <http://www.unicode.org/reports/tr44/#UnicodeData.txt>.
#[derive(Deserialize)]
struct UnicodeDataLine<'a> {
    code_point: CodePoint,
    name: &'a str,
    _general_category: &'a str,
    _canonical_combining_class: &'a str,
    _bidi_class: &'a str,
    _decomposition: &'a str,
    _decimal_value: de::IgnoredAny,
    _digit_value: &'a str,
    _numeric_value: &'a str,
    _bidi_mirrored: de::IgnoredAny,
    _unicode_1_name: de::IgnoredAny,
    _iso_comment: de::IgnoredAny,
    _simple_uppercase_mapping: de::IgnoredAny,
    _simple_lowercase_mapping: de::IgnoredAny,
    _simple_titlecase_mapping: de::IgnoredAny,
}

/// A line of `NameAliases.txt`.
///
/// See <http://www.unicode.org/reports/tr44/#NameAliases.txt>.
#[derive(Deserialize)]
struct NameAlias<'a> {
    code_point: CodePoint,
    value: &'a str,
    ty: AliasType,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum AliasType {
    Correction,
    Control,
    Alternate,
    Figment,
    Abbreviation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CodePoint(u32);

impl CodePoint {
    fn as_printable(self) -> Option<char> {
        char::from_u32(self.0).filter(|c| !c.is_control())
    }
}

impl<'de> Deserialize<'de> for CodePoint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = CodePoint;
            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a Unicode code point")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let code_point = u32::from_str_radix(v, 16)
                    .map_err(|_| E::invalid_type(de::Unexpected::Str(v), &self))?;

                if code_point >= 0x10FFFF {
                    return Err(E::custom(format_args!(
                        "{code_point:04X} is not a valid code point"
                    )));
                }

                Ok(CodePoint(code_point))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

impl Display for CodePoint {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:04X}", self.0)
    }
}

struct CodePointRange {
    start: CodePoint,
    end: Option<CodePoint>,
}
impl<'de> Deserialize<'de> for CodePointRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = CodePointRange;
            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a code point or range of code points")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let mut range = v.splitn(2, "..");
                let start = CodePoint::deserialize(range.next().unwrap().into_deserializer())?;
                let end = range
                    .next()
                    .map(|end| CodePoint::deserialize(end.into_deserializer()))
                    .transpose()?;
                Ok(CodePointRange { start, end })
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

struct BorrowedStr<'de>(&'de str);
impl<'de, E: de::Error> IntoDeserializer<'de, E> for BorrowedStr<'de> {
    type Deserializer = de::value::BorrowedStrDeserializer<'de, E>;
    fn into_deserializer(self) -> Self::Deserializer {
        de::value::BorrowedStrDeserializer::new(self.0)
    }
}

fn load_text(agent: &ureq::Agent, url_or_path: &str) -> anyhow::Result<String> {
    if url_or_path.starts_with("http://") || url_or_path.starts_with("https://") {
        agent
            .get(url_or_path)
            .call()
            .map_err(anyhow::Error::new)
            .and_then(|res| Ok(res.into_string()?))
            .with_context(|| format!("failed to download file <{url_or_path}>"))
    } else {
        fs::read_to_string(url_or_path)
            .with_context(|| format!("failed to read in file {url_or_path}"))
    }
}

fn write_file(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> anyhow::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    fs::write(path, data).with_context(|| format!("failed to write to {}", path.display()))?;

    Ok(())
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
