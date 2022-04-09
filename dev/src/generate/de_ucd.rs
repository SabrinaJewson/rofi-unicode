//! Deserialization of files from the UCD with Serde.

use ::{
    serde::de::{self, Deserialize, Deserializer, Error as _},
    std::{
        error::Error as StdError,
        fmt::{self, Display, Formatter},
        iter,
        marker::PhantomData,
        str,
    },
};
pub(crate) fn lines<'file, Line: Deserialize<'file>>(file: &'file str) -> Lines<'file, Line> {
    Lines {
        inner: file.lines().enumerate(),
        line: PhantomData,
    }
}

pub(crate) struct Lines<'file, Line> {
    inner: iter::Enumerate<str::Lines<'file>>,
    line: PhantomData<fn() -> Line>,
}

impl<'file, Line: Deserialize<'file>> Iterator for Lines<'file, Line> {
    type Item = Result<Line, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let (i, line) = loop {
            let (i, line) = self.inner.next()?;
            let line = line.split('#').next().unwrap();
            if !line.is_empty() {
                break (i, line);
            }
        };
        Some(line_from_str(line).map_err(|inner| Error { line: i + 1, inner }))
    }
}

#[derive(Debug)]
pub(crate) struct Error {
    line: usize,
    inner: UcdLineError,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "error deserializing UCD file on line {}", self.line)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.inner)
    }
}

fn line_from_str<'line, Line: Deserialize<'line>>(line: &'line str) -> Result<Line, UcdLineError> {
    Line::deserialize(UcdLineDeserializer { line })
}

struct UcdLineDeserializer<'de> {
    line: &'de str,
}

impl<'de> Deserializer<'de> for UcdLineDeserializer<'de> {
    type Error = UcdLineError;

    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        struct SeqAccess<'de> {
            field: usize,
            parts: str::Split<'de, char>,
        }

        impl<'de> de::SeqAccess<'de> for &mut SeqAccess<'de> {
            type Error = de::value::Error;

            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
            where
                T: de::DeserializeSeed<'de>,
            {
                match self.parts.next() {
                    Some(part) => {
                        let val =
                            seed.deserialize(de::value::BorrowedStrDeserializer::new(part.trim()))?;
                        self.field += 1;
                        Ok(Some(val))
                    }
                    None => Ok(None),
                }
            }
        }

        let mut access = SeqAccess {
            field: 0,
            parts: self.line.split(';'),
        };

        let value = visitor
            .visit_seq(&mut access)
            .map_err(|inner| UcdLineError {
                field: Some(access.field),
                inner,
            })?;

        let remaining = access.parts.count();
        if remaining != 0 {
            let got = access.field + remaining;
            let expected = if access.field == 1 {
                "1 field".to_owned()
            } else {
                format!("{} fields", access.field)
            };
            return Err(UcdLineError::invalid_length(got, &&*expected));
        }

        Ok(value)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

#[derive(Debug)]
struct UcdLineError {
    field: Option<usize>,
    inner: de::value::Error,
}

impl UcdLineError {
    fn no_field(inner: de::value::Error) -> Self {
        Self { field: None, inner }
    }
}

impl de::Error for UcdLineError {
    fn custom<T: Display>(msg: T) -> Self {
        Self::no_field(de::value::Error::custom(msg))
    }
}

impl Display for UcdLineError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(field) = self.field {
            write!(f, "error deserializing field {field}")
        } else {
            Display::fmt(&self.inner, f)
        }
    }
}

impl StdError for UcdLineError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        if self.field.is_some() {
            Some(&self.inner)
        } else {
            self.inner.source()
        }
    }
}
