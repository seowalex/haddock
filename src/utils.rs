use std::{
    env,
    error::Error,
    fmt::{self, Formatter},
    hash::Hash,
    marker::PhantomData,
    str::FromStr,
};

use anyhow::Result;
use console::{style, StyledObject};
use indexmap::IndexSet;
use once_cell::sync::Lazy;
use serde::{
    de::{self, SeqAccess, Visitor},
    Deserializer, Serialize, Serializer,
};
use serde_with::{
    de::DeserializeAsWrap, formats::Separator, ser::SerializeAsWrap, DeserializeAs, SerializeAs,
};
use sha2::{Digest as _, Sha256};

pub(crate) static STYLED_WARNING: Lazy<StyledObject<&str>> =
    Lazy::new(|| style("Warning:").yellow().bold());

pub(crate) fn parse_colon_delimited<T, U>(s: &str) -> Result<(Option<T>, U)>
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    if let Some(pos) = s.find(':') {
        Ok((Some(s[..pos].parse()?), s[pos + 1..].parse()?))
    } else {
        Ok((None, s.parse()?))
    }
}

macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

pub(crate) use regex;

pub(crate) trait Digest {
    fn digest(&self) -> String;
}

impl<T> Digest for T
where
    T: Serialize,
{
    fn digest(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(serde_yaml::to_string(self).unwrap().as_bytes())
        )
    }
}

pub(crate) struct DisplayFromAny;

impl<'de, T> DeserializeAs<'de, T> for DisplayFromAny
where
    T: From<String>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AnyVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for AnyVisitor<T>
        where
            T: From<String>,
        {
            type Value = T;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str("a displayable type")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(T::from(v.to_string()))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(T::from(v.to_string()))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(T::from(v.to_string()))
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(T::from(v.to_string()))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(T::from(v.to_string()))
            }
        }

        deserializer.deserialize_any(AnyVisitor(PhantomData))
    }
}

impl<T> SerializeAs<T> for DisplayFromAny
where
    T: Serialize,
{
    fn serialize_as<S>(source: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        source.serialize(serializer)
    }
}

pub(crate) struct DuplicateInsertsLastWinsSet<T>(PhantomData<T>);

impl<'de, T, U> DeserializeAs<'de, IndexSet<T>> for DuplicateInsertsLastWinsSet<U>
where
    T: Eq + Hash,
    U: DeserializeAs<'de, T>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<IndexSet<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SeqVisitor<T, U>(PhantomData<(T, U)>);

        impl<'de, T, U> Visitor<'de> for SeqVisitor<T, U>
        where
            T: Eq + Hash,
            U: DeserializeAs<'de, T>,
        {
            type Value = IndexSet<T>;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str("a sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Self::Value::with_capacity(seq.size_hint().unwrap_or(0).min(4096));

                while let Some(value) = seq
                    .next_element()?
                    .map(DeserializeAsWrap::<T, U>::into_inner)
                {
                    values.replace(value);
                }

                Ok(values)
            }
        }

        deserializer.deserialize_seq(SeqVisitor::<T, U>(PhantomData))
    }
}

impl<T, U> SerializeAs<IndexSet<T>> for DuplicateInsertsLastWinsSet<U>
where
    T: Eq + Hash,
    U: SerializeAs<T>,
{
    fn serialize_as<S>(source: &IndexSet<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(source.iter().map(|item| SerializeAsWrap::<T, U>::new(item)))
    }
}

pub(crate) struct PathSeparator;

impl Separator for PathSeparator {
    fn separator() -> &'static str {
        Box::leak(
            env::var("COMPOSE_PATH_SEPARATOR")
                .unwrap_or_else(|_| {
                    String::from(if cfg!(unix) {
                        ":"
                    } else if cfg!(windows) {
                        ";"
                    } else {
                        unreachable!()
                    })
                })
                .into_boxed_str(),
        )
    }
}
