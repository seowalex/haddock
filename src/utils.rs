use serde_with::formats::Separator;
use serde_yaml::Value;
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

pub(crate) trait MergeOption<T> {
    fn merge(&mut self, other: Self, f: fn(&mut T, T));

    fn merge_one(&mut self, other: Self);

    fn merge_many<A>(&mut self, other: Self)
    where
        T: Extend<A> + IntoIterator<Item = A>;
}

impl<T> MergeOption<T> for Option<T> {
    fn merge(&mut self, other: Self, f: fn(&mut T, T)) {
        match (self, other) {
            (Some(a), Some(b)) => f(a, b),
            (a @ None, b @ Some(_)) => *a = b,
            _ => {}
        }
    }

    fn merge_one(&mut self, other: Self) {
        if other.is_some() {
            *self = other;
        }
    }

    fn merge_many<A>(&mut self, other: Self)
    where
        T: Extend<A> + IntoIterator<Item = A>,
    {
        match (self, other) {
            (Some(a), Some(b)) => a.extend(b),
            (a @ None, b @ Some(_)) => *a = b,
            _ => {}
        }
    }
}

pub(crate) trait MergeValue {
    fn merge(&mut self, other: Self);
}

impl MergeValue for Value {
    fn merge(&mut self, other: Self) {
        match (self, other) {
            (left @ Value::Mapping(_), Value::Mapping(right)) => {
                let left = left.as_mapping_mut().unwrap();

                for (key, value) in right {
                    if left.contains_key(&key) {
                        if let Some(key) = key.as_str() {
                            if key == "command" || key == "entrypoint" {
                                left[&key] = value;
                                continue;
                            }
                        }

                        left[&key].merge(value);
                    } else {
                        left.insert(key, value);
                    }
                }
            }
            (Value::Sequence(left), Value::Sequence(right)) => {
                left.extend(right);
            }
            (_, Value::Null) => {}
            (left, right) => *left = right,
        }
    }
}
