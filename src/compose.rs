mod parser;
mod types;

use anyhow::{anyhow, bail, Context, Error, Result};
use clap::crate_name;
use indexmap::IndexSet;
use itertools::Itertools;
use serde_yaml::Value;
use std::{env, fs};
use yansi::Paint;

use crate::{config::Config, utils::regex};
use parser::{State, Token, Var};
use types::Compose;

fn evaluate(tokens: Vec<Token>) -> Result<String> {
    tokens
        .into_iter()
        .map(|token| match token {
            Token::Str(string) => Ok(string),
            Token::Var(name, var) => match var {
                Some(Var::Default(state, tokens)) => match state {
                    State::Set => env::var(name),
                    State::SetAndNonEmpty => env::var(name).and_then(|var| {
                        if var.is_empty() {
                            Err(env::VarError::NotPresent)
                        } else {
                            Ok(var)
                        }
                    }),
                }
                .or_else(|_| evaluate(tokens)),
                Some(Var::Err(state, tokens)) => match state {
                    State::Set => env::var(&name),
                    State::SetAndNonEmpty => env::var(&name).and_then(|var| {
                        if var.is_empty() {
                            Err(env::VarError::NotPresent)
                        } else {
                            Ok(var)
                        }
                    }),
                }
                .or_else(|_| {
                    evaluate(tokens).and_then(|err| {
                        if err.is_empty() {
                            bail!("Required variable \"{name}\" is missing a value");
                        }

                        bail!("Required variable \"{name}\" is missing a value: {err}");
                    })
                }),
                Some(Var::Replace(state, tokens)) => match state {
                    State::Set => env::var(name),
                    State::SetAndNonEmpty => env::var(name).and_then(|var| {
                        if var.is_empty() {
                            Err(env::VarError::NotPresent)
                        } else {
                            Ok(var)
                        }
                    }),
                }
                .map_or_else(|_| Ok(String::new()), |_| evaluate(tokens)),
                None => Ok(env::var(&name).unwrap_or_else(|_| {
                    eprintln!(
                        "{} The \"{name}\" variable is not set, defaulting to a blank string",
                        Paint::yellow("Warning:").bold()
                    );

                    String::new()
                })),
            },
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|tokens| tokens.join(""))
}

fn interpolate(value: &Value) -> Result<Value> {
    if let Some(value) = value.as_str() {
        parser::parse(value).and_then(evaluate).map(Value::String)
    } else if let Some(values) = value.as_sequence() {
        values.iter().map(interpolate).collect::<Result<_>>()
    } else if let Some(values) = value.as_mapping() {
        values
            .iter()
            .map(|(key, value)| {
                interpolate(value)
                    .with_context(|| key.as_str().unwrap().to_string())
                    .map(|value| (key.clone(), value))
            })
            .collect::<Result<_>>()
            .map(Value::Mapping)
    } else {
        Ok(value.clone())
    }
}

pub(crate) fn parse(config: Config) -> Result<Compose> {
    let contents = config
        .files
        .into_iter()
        .map(|path| {
            fs::read_to_string(&path)
                .with_context(|| format!("{path} not found"))
                .map(|content| (path, content))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let files = contents
        .into_iter()
        .enumerate()
        .map(|(i, (path, content))| {
            serde_yaml::from_str(&content)
                .map_err(Error::from)
                .map(|mut content: Value| {
                    if let Some(values) = content.as_mapping_mut() {
                        let name = if config.project_name.is_some() {
                            config.project_name.clone()
                        } else if let Some((_, n)) =
                            values.into_iter().find(|(key, _)| *key == "name")
                        {
                            n.as_str()
                                .map(ToString::to_string)
                                .or_else(|| n.as_bool().map(|n| n.to_string()))
                                .or_else(|| n.as_u64().map(|n| n.to_string()))
                                .or_else(|| n.as_i64().map(|n| n.to_string()))
                                .or_else(|| n.as_f64().map(|n| n.to_string()))
                                .or_else(|| Some(String::new()))
                        } else if i == 0 {
                            Some(String::new())
                        } else {
                            None
                        };

                        if let Some(mut name) = name {
                            let re = regex!(r"^[^a-zA-Z0-9]+|[^a-zA-Z0-9_.-]");
                            name = re.replace_all(&name, "").to_ascii_lowercase();

                            if name.is_empty() {
                                name = re
                                    .replace_all(
                                        &env::current_dir()
                                            .ok()
                                            .and_then(|name| {
                                                name.file_name()
                                                    .map(|name| name.to_string_lossy().to_string())
                                            })
                                            .unwrap_or_else(|| crate_name!().to_string()),
                                        "",
                                    )
                                    .to_ascii_lowercase();
                            }

                            env::set_var("COMPOSE_PROJECT_NAME", &name);
                            values.insert(Value::String(String::from("name")), Value::String(name));
                        }
                    }

                    (path, content)
                })
        })
        .map(|content| {
            content.and_then(|(path, content)| {
                interpolate(&content)
                    .map_err(|err| match err.chain().collect::<Vec<_>>().split_last() {
                        Some((err, props)) => {
                            anyhow!("{}: {err}", props.iter().join("."))
                        }
                        None => err,
                    })
                    .map(|content| (path, content))
            })
        })
        .map(|content| {
            content.and_then(|(path, content)| {
                serde_yaml::to_string(&content)
                    .map_err(Error::from)
                    .map(|content| (path, content))
            })
        })
        .map(|content| {
            content.and_then(|(path, content)| {
                let mut unused = IndexSet::new();

                serde_ignored::deserialize(serde_yaml::Deserializer::from_str(&content), |path| {
                    unused.insert(path.to_string());
                })
                .with_context(|| format!("{path} does not follow the Compose specification"))
                .map(|file: Compose| (path, file, unused))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut combined_file = Compose::new();

    for (path, file, unused) in files {
        if !unused.is_empty() {
            eprintln!(
                "{} Unsupported/unknown properties in {path}: {}",
                Paint::yellow("Warning:").bold(),
                unused.into_iter().join(", ")
            );
        }

        combined_file.merge(file);
    }

    for (name, service) in &combined_file.services {
        if service.build.is_none() && service.image.is_none() {
            bail!("Service \"{name}\" has neither an image nor a build context specified");
        }

        if service.network_mode.as_deref().unwrap_or_default() == "host" && service.ports.is_some()
        {
            bail!("Service \"{name}\" cannot have port mappings due to host network mode");
        }
    }

    if let Some(networks) = &combined_file.networks {
        for (name, network) in networks {
            if let Some(network) = network {
                if network.external.unwrap_or_default()
                    && (network.driver.is_some()
                        || network.driver_opts.is_some()
                        || network.enable_ipv6.is_some()
                        || network.ipam.is_some()
                        || network.internal.is_some()
                        || network.labels.is_some())
                {
                    bail!("Conflicting parameters specified for network \"{name}\"");
                }
            }
        }
    }

    if let Some(volumes) = &combined_file.volumes {
        for (name, volume) in volumes {
            if let Some(volume) = volume {
                if volume.external.unwrap_or_default()
                    && (volume.driver.is_some()
                        || volume.driver_opts.is_some()
                        || volume.labels.is_some())
                {
                    bail!("Conflicting parameters specified for volume \"{name}\"");
                }
            }
        }
    }

    if let Some(configs) = &combined_file.configs {
        for (name, config) in configs {
            if config.external.unwrap_or_default() && config.file.is_some() {
                bail!("Conflicting parameters specified for config \"{name}\"");
            }
        }
    }

    if let Some(secrets) = &combined_file.secrets {
        for (name, secret) in secrets {
            if secret.external.unwrap_or_default()
                && (secret.file.is_some() || secret.environment.is_some())
            {
                bail!("Conflicting parameters specified for secret \"{name}\"");
            }
        }
    }

    Ok(combined_file)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_yaml::Value;

    use super::*;

    #[test]
    fn simple_named() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(&Value::String(String::from("$VAR")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn simple_named_missing() {
        let result = temp_env::with_var("VAR", None::<&str>, || {
            interpolate(&Value::String(String::from("pre $VAR post")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("pre  post"))));
    }

    #[test]
    fn braced_named() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(&Value::String(String::from("${VAR}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn braced_named_text() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(&Value::String(String::from("pre ${VAR} post")))
        });

        assert_eq!(
            result.ok(),
            Some(Value::String(String::from("pre woop post")))
        );
    }

    #[test]
    fn default_named() {
        let result = temp_env::with_var("VAR", None::<&str>, || {
            interpolate(&Value::String(String::from("${VAR-default}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("default"))));
    }

    #[test]
    fn no_default_named() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(&Value::String(String::from("${VAR-default}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn default_pattern() {
        let result = temp_env::with_var("DEF", Some("woop"), || {
            interpolate(&Value::String(String::from("${VAR-$DEF}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn default_named_no_empty() {
        let result = temp_env::with_var("VAR", Some(""), || {
            interpolate(&Value::String(String::from("${VAR:-default}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("default"))));
    }

    #[test]
    fn no_default_named_no_empty() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(&Value::String(String::from("${VAR:-default}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn default_pattern_no_empty() {
        let result = temp_env::with_vars(vec![("VAR", Some("")), ("DEF", Some("woop"))], || {
            interpolate(&Value::String(String::from("${VAR:-$DEF}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn error_named() {
        let result = temp_env::with_var("VAR", None::<&str>, || {
            interpolate(&Value::String(String::from("${VAR?msg}")))
        });

        assert_eq!(
            result.err().map(|err| err.to_string()),
            Some(String::from(
                "Required variable \"VAR\" is missing a value: msg"
            ))
        );
    }

    #[test]
    fn error_named_no_empty() {
        let result = temp_env::with_var("VAR", Some(""), || {
            interpolate(&Value::String(String::from("${VAR:?msg}")))
        });

        assert_eq!(
            result.err().map(|err| err.to_string()),
            Some(String::from(
                "Required variable \"VAR\" is missing a value: msg"
            ))
        );
    }

    #[test]
    fn error_no_message() {
        let result = temp_env::with_var("VAR", None::<&str>, || {
            interpolate(&Value::String(String::from("${VAR?}")))
        });

        assert_eq!(
            result.err().map(|err| err.to_string()),
            Some(String::from("Required variable \"VAR\" is missing a value"))
        );
    }

    #[test]
    fn error_no_message_no_empty() {
        let result = temp_env::with_var("VAR", Some(""), || {
            interpolate(&Value::String(String::from("${VAR:?}")))
        });

        assert_eq!(
            result.err().map(|err| err.to_string()),
            Some(String::from("Required variable \"VAR\" is missing a value"))
        );
    }

    #[test]
    fn replacement_named() {
        let result = temp_env::with_var("VAR", Some(""), || {
            interpolate(&Value::String(String::from("${VAR+replacement}")))
        });

        assert_eq!(
            result.ok(),
            Some(Value::String(String::from("replacement")))
        );
    }

    #[test]
    fn no_replacement_named() {
        let result = temp_env::with_var("VAR", None::<&str>, || {
            interpolate(&Value::String(String::from("${VAR+replacement}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::new())));
    }

    #[test]
    fn replacement_named_no_empty() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(&Value::String(String::from("${VAR:+replacement}")))
        });

        assert_eq!(
            result.ok(),
            Some(Value::String(String::from("replacement")))
        );
    }

    #[test]
    fn no_replacement_named_no_empty() {
        let result = temp_env::with_var("VAR", Some(""), || {
            interpolate(&Value::String(String::from("${VAR:+replacement}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::new())));
    }
}
