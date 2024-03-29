use std::{
    env,
    error::Error,
    fmt::{self, Formatter},
    marker::PhantomData,
    str::FromStr,
};

use anyhow::{anyhow, Result};
use console::{style, StyledObject};
use once_cell::sync::Lazy;
use serde::{
    de::{self, Visitor},
    Deserializer, Serialize, Serializer,
};
use serde_with::{formats::Separator, DeserializeAs, SerializeAs};
use sha2::{Digest as _, Sha256};

pub(crate) static STYLED_WARNING: Lazy<StyledObject<&str>> =
    Lazy::new(|| style("Warning:").yellow().bold());

pub(crate) fn parse_container_path<T, U>(s: &str) -> Result<(Option<T>, U)>
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

pub(crate) fn parse_key_val<T, U>(s: &str) -> Result<(T, U)>
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let pos = s.find('=').ok_or_else(|| {
        anyhow!(
            "no '{}' found in '{}'",
            style("=").yellow(),
            style(s).yellow()
        )
    })?;

    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

pub(crate) fn parse_key_val_opt<T, U>(s: &str) -> Result<(T, Option<U>)>
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    if let Some(pos) = s.find('=') {
        Ok((s[..pos].parse()?, Some(s[pos + 1..].parse()?)))
    } else {
        Ok((s.parse()?, None))
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
