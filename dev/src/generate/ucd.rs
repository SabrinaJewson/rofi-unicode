use {
    super::{de_ucd, with_glib_markup_escaped, Content, Item, Items, Opts},
    ::{
        anyhow::Context as _,
        serde::{
            de::{self, Deserializer, IntoDeserializer},
            Deserialize,
        },
        std::fmt::{self, Display, Formatter, Write as _},
    },
};

pub(super) fn generate(opts: &Opts<'_>) -> anyhow::Result<()> {
    const UNICODE_DATA: &str = "Public/UCD/latest/ucd/UnicodeData.txt";
    const NAME_ALIASES: &str = "Public/UCD/latest/ucd/NameAliases.txt";

    let unicode_data = opts.load_text(UNICODE_DATA)?;
    let mut unicode_data = de_ucd::lines::<UnicodeDataLine<'_>>(&unicode_data)
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse {UNICODE_DATA}"))?;

    let name_aliases = opts.load_text(NAME_ALIASES)?;
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

    opts.write_ron("codepoints.ron", generate_codepoints(&data)?)?;

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

fn generate_codepoints(data: &UnicodeData<'_>) -> anyhow::Result<Items> {
    let mut items = Vec::new();

    let mut name_aliases = data.name_aliases.iter().fuse().peekable();
    for &UnicodeDataLine {
        code_point, name, ..
    } in &data.unicode_data
    {
        if let Some(next_alias) = name_aliases.peek() {
            anyhow::ensure!(
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
        let scalar_value = match char::from_u32(code_point.0) {
            Some(scalar_value) => scalar_value,
            None => continue,
        };

        let printable = if scalar_value.is_control() {
            ' '
        } else {
            scalar_value
        };

        let name_unescaped = format!("U+{code_point}\t{printable}\t{corrected_name}");
        let mut name = with_glib_markup_escaped(&*name_unescaped, |s| s.to_owned());
        if !alternate_names.is_empty() {
            with_glib_markup_escaped(&*alternate_names, |alternate_names| {
                write!(name, " (<small>{alternate_names}</small>)").unwrap();
            });
        }

        items.push(Item {
            name,
            content: Content::Text(scalar_value.to_string()),
        });
    }

    Ok(Items::from_direct(items))
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
