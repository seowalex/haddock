use serde_with::formats::Separator;
use std::env;

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
