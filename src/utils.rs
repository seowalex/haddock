use either::Either;
use indexmap::IndexSet;
use path_absolutize::Absolutize;
use serde::{
    de::{Error, SeqAccess, Visitor},
    Deserializer, Serialize, Serializer,
};
use serde_with::{
    de::DeserializeAsWrap, formats::Separator, ser::SerializeAsWrap, DeserializeAs, SerializeAs,
};
use std::{
    env, fmt,
    hash::Hash,
    marker::PhantomData,
    path::{Path, PathBuf},
};

macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

pub(crate) use regex;

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

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Self::Value::with_capacity(seq.size_hint().unwrap_or(0).min(4096));

                while let Some(value) = seq
                    .next_element()?
                    .map(|v: DeserializeAsWrap<T, U>| v.into_inner())
                {
                    values.replace(value);
                }

                Ok(values)
            }
        }

        let visitor = SeqVisitor::<T, U>(PhantomData);
        deserializer.deserialize_seq(visitor)
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

pub(crate) trait Merge<T> {
    fn merge<A>(&mut self, other: Self)
    where
        T: Extend<A> + IntoIterator<Item = A>;

    fn merge_one(&mut self, other: Self);
}

impl<T> Merge<T> for Option<T> {
    fn merge<A>(&mut self, other: Self)
    where
        T: Extend<A> + IntoIterator<Item = A>,
    {
        match (self, other) {
            (Some(a), Some(b)) => a.extend(b),
            (a @ None, b @ Some(_)) => *a = b,
            _ => {}
        }
    }

    fn merge_one(&mut self, other: Self) {
        if other.is_some() {
            *self = other;
        }
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

pub(crate) struct EitherPathBufOrString;

impl<'de> DeserializeAs<'de, Either<PathBuf, String>> for EitherPathBufOrString {
    fn deserialize_as<D>(deserializer: D) -> Result<Either<PathBuf, String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AnyVisitor;

        impl<'de> Visitor<'de> for AnyVisitor {
            type Value = Either<PathBuf, String>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a displayable type")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(Either::Right(v.to_string()))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(Either::Right(v.to_string()))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(Either::Right(v.to_string()))
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(Either::Right(v.to_string()))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                if v.starts_with('/') || v.starts_with('.') {
                    Path::new(v)
                        .absolutize()
                        .map_err(Error::custom)
                        .map(|path| Either::Left(path.to_path_buf()))
                } else {
                    Ok(Either::Right(v.to_string()))
                }
            }
        }

        let visitor = AnyVisitor;
        deserializer.deserialize_any(visitor)
    }
}

impl SerializeAs<Either<PathBuf, String>> for EitherPathBufOrString {
    fn serialize_as<S>(source: &Either<PathBuf, String>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        either::for_both!(source, source => source.serialize(serializer))
    }
}

pub(crate) struct DisplayFromAny;

impl<'de> DeserializeAs<'de, PathBuf> for DisplayFromAny {
    fn deserialize_as<D>(deserializer: D) -> Result<PathBuf, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AnyVisitor;

        impl<'de> Visitor<'de> for AnyVisitor {
            type Value = PathBuf;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a displayable type")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(PathBuf::from(v.to_string()))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(PathBuf::from(v.to_string()))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(PathBuf::from(v.to_string()))
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(PathBuf::from(v.to_string()))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(PathBuf::from(v))
            }
        }

        let visitor = AnyVisitor;
        deserializer.deserialize_any(visitor)
    }
}

impl<'de> DeserializeAs<'de, String> for DisplayFromAny {
    fn deserialize_as<D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AnyVisitor;

        impl<'de> Visitor<'de> for AnyVisitor {
            type Value = String;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a displayable type")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(v.to_string())
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(v.to_string())
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(v.to_string())
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(v.to_string())
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(v.to_string())
            }
        }

        let visitor = AnyVisitor;
        deserializer.deserialize_any(visitor)
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
