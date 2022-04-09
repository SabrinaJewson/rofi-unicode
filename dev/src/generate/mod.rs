use ::{
    anyhow::Context as _,
    serde::{
        de::{self, Deserializer, IntoDeserializer},
        Deserialize,
    },
    std::{
        env,
        fmt::{self, Display, Formatter, Write as _},
        fs,
        path::PathBuf,
        str,
    },
};

mod de_ucd;

pub(crate) fn generate() -> anyhow::Result<()> {
    let agent = ureq::agent();

    const UNICODE_DATA: &str = "https://www.unicode.org/Public/UCD/latest/ucd/UnicodeData.txt";
    let unicode_data = download_text(&agent, UNICODE_DATA)?;
    let lines = de_ucd::lines::<UnicodeDataLine<'_>>(&unicode_data)
        .collect::<Result<Vec<_>, _>>()
        .context("failed to parse UnicodeData.txt")?;

    let mut result = "use crate::Entry;".to_owned();

    result.push_str("pub(super) const ENTRIES: &[Entry] = &[");
    for line in lines {
        if line.name.starts_with('<') {
            continue;
        }

        let data = line.code_point;
        let displayed = format!(
            "U+{}\t{}\t{}",
            line.code_point,
            line.code_point.as_printable().unwrap_or(' '),
            line.name,
        );
        let complete_with = if line.name.is_empty() {
            format!("U+{}", line.code_point)
        } else {
            line.name.to_owned()
        };
        write!(
            result,
            "Entry {{\
                data: \"\\u{{{data}}}\",\
                complete_with: \"{complete_with}\",\
                displayed: \"{displayed}\",\
            }},",
            data = data,
            complete_with = complete_with.escape_default(),
            displayed = displayed.escape_default(),
        )
        .unwrap();
    }
    result.push_str("];");

    let mut generated_file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    generated_file_path.pop();
    generated_file_path.push("src");
    generated_file_path.push("generated.rs");
    fs::write(generated_file_path, result).context("failed to write generated.rs")?;

    println!("Successfully wrote to generated.rs");

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

fn download_text(agent: &ureq::Agent, url: &str) -> anyhow::Result<String> {
    agent
        .get(url)
        .call()
        .map_err(anyhow::Error::new)
        .and_then(|res| Ok(res.into_string()?))
        .with_context(|| format!("failed to download file <{url}>"))
}
