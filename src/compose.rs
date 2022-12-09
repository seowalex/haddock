mod parser;
mod types;

use anyhow::{anyhow, bail, Context, Error, Result};
use indexmap::IndexSet;
use itertools::Itertools;
use serde_yaml::Value;
use std::{env, fs};
use yansi::Paint;

use crate::Config;
use types::Compose;

fn evaluate(tokens: Vec<parser::Token>) -> Result<String> {
    tokens
        .into_iter()
        .map(|token| match token {
            parser::Token::Str(string) => Ok(string),
            parser::Token::Var(name, var) => match var {
                Some(parser::Var::Default(state, tokens)) => match state {
                    parser::State::Set => env::var(name),
                    parser::State::SetAndNonEmpty => env::var(name).and_then(|var| {
                        if var.is_empty() {
                            Err(env::VarError::NotPresent)
                        } else {
                            Ok(var)
                        }
                    }),
                }
                .or_else(|_| evaluate(tokens)),
                Some(parser::Var::Err(state, tokens)) => match state {
                    parser::State::Set => env::var(&name),
                    parser::State::SetAndNonEmpty => env::var(&name).and_then(|var| {
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
                            bail!("Required variable \"{name}\" is missing a value")
                        } else {
                            bail!("Required variable \"{name}\" is missing a value: {err}")
                        }
                    })
                }),
                Some(parser::Var::Replace(state, tokens)) => {
                    if match state {
                        parser::State::Set => env::var(name),
                        parser::State::SetAndNonEmpty => env::var(name).and_then(|var| {
                            if var.is_empty() {
                                Err(env::VarError::NotPresent)
                            } else {
                                Ok(var)
                            }
                        }),
                    }
                    .is_ok()
                    {
                        evaluate(tokens)
                    } else {
                        Ok(String::new())
                    }
                }
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

fn interpolate(mut value: Value) -> Result<Value> {
    if let Some(value) = value.as_str() {
        return parser::parse(value).and_then(evaluate).map(Value::String);
    } else if let Some(values) = value.as_sequence_mut() {
        for value in values {
            *value = interpolate(value.to_owned())?;
        }
    } else if let Some(values) = value.as_mapping_mut() {
        for (key, value) in values.into_iter() {
            *value = interpolate(value.to_owned())
                .with_context(|| key.as_str().unwrap_or_default().to_owned())?;
        }
    }

    Ok(value)
}

pub(crate) fn parse(config: Config) -> Result<Compose> {
    let contents = match config.file {
        Some(paths) => paths
            .into_iter()
            .map(|path| {
                fs::read_to_string(&path)
                    .with_context(|| format!("{path} not found"))
                    .map(|content| (path, content))
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => vec![fs::read_to_string("compose.yaml")
            .map(|content| (String::from("compose.yaml"), content))
            .or_else(|_| {
                fs::read_to_string("compose.yml")
                    .map(|content| (String::from("compose.yml"), content))
            })
            .or_else(|_| {
                fs::read_to_string("docker-compose.yaml")
                    .map(|content| (String::from("docker-compose.yaml"), content))
            })
            .or_else(|_| {
                fs::read_to_string("docker-compose.yml")
                    .map(|content| (String::from("docker-compose.yml"), content))
            })
            .context("compose.yaml not found")?],
    };
    let files = contents
        .into_iter()
        .map(|(path, content)| {
            serde_yaml::from_str(&content)
                .map(|mut content: Value| {
                    if let Some(values) = content.as_mapping_mut() {
                        if let Some((_, name)) = values.into_iter().find(|(key, _)| *key == "name")
                        {
                            if name.is_string() {
                                if let Ok(interpolated_name) = interpolate(name.to_owned()) {
                                    *name = interpolated_name;
                                }

                                env::set_var("COMPOSE_PROJECT_NAME", name.as_str().unwrap());
                            } else if name.is_bool() {
                                env::set_var(
                                    "COMPOSE_PROJECT_NAME",
                                    name.as_bool().unwrap().to_string(),
                                );
                            } else if name.is_u64() {
                                env::set_var(
                                    "COMPOSE_PROJECT_NAME",
                                    name.as_u64().unwrap().to_string(),
                                );
                            } else if name.is_i64() {
                                env::set_var(
                                    "COMPOSE_PROJECT_NAME",
                                    name.as_i64().unwrap().to_string(),
                                );
                            } else if name.is_f64() {
                                env::set_var(
                                    "COMPOSE_PROJECT_NAME",
                                    name.as_f64().unwrap().to_string(),
                                );
                            }
                        }
                    }

                    (path, content)
                })
                .map_err(Error::from)
        })
        .map(|content| {
            content.and_then(|(path, content)| {
                interpolate(content)
                    .map(|content| (path, content))
                    .map_err(|err| match err.chain().collect::<Vec<_>>().split_last() {
                        Some((err, props)) => {
                            anyhow!("{}: {err}", props.iter().join("."))
                        }
                        None => err,
                    })
            })
        })
        .map(|content| {
            content.and_then(|(path, content)| {
                serde_yaml::to_string(&content)
                    .map(|content| (path, content))
                    .map_err(Error::from)
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
        for (name, service) in &file.services {
            if service.build.is_none() && service.image.is_none() {
                bail!(
                    "{path}: service \"{name}\" has neither an image nor a build context specified"
                );
            }

            if service.network_mode.as_deref().unwrap_or_default() == "host"
                && service.ports.is_some()
            {
                bail!(
                    "{path}: service \"{name}\" cannot have port mappings due to host network mode"
                );
            }
        }

        if let Some(networks) = &file.networks {
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
                        bail!("{path}: conflicting parameters for network \"{name}\"");
                    }
                }
            }
        }

        if let Some(volumes) = &file.volumes {
            for (name, volume) in volumes {
                if let Some(volume) = volume {
                    if volume.external.unwrap_or_default()
                        && (volume.driver.is_some()
                            || volume.driver_opts.is_some()
                            || volume.labels.is_some())
                    {
                        bail!("{path}: conflicting parameters for volume \"{name}\"");
                    }
                }
            }
        }

        if let Some(configs) = &file.configs {
            for (name, config) in configs {
                if config.external.unwrap_or_default() && config.file.is_some() {
                    bail!("{path}: conflicting parameters for config \"{name}\"");
                }
            }
        }

        if let Some(secrets) = &file.secrets {
            for (name, secret) in secrets {
                if secret.external.unwrap_or_default()
                    && (secret.file.is_some() || secret.environment.is_some())
                {
                    bail!("{path}: conflicting parameters for secret \"{name}\"");
                }
            }
        }

        if !unused.is_empty() {
            eprintln!(
                "{} Unsupported/unknown properties in {path}: {}",
                Paint::yellow("Warning:").bold(),
                unused.into_iter().join(", ")
            );
        }

        combined_file.version = file.version;
        combined_file.name = file.name;
        combined_file.services.extend(file.services);

        match (&mut combined_file.networks, file.networks) {
            (Some(combined_networks), Some(networks)) => combined_networks.extend(networks),
            (combined_networks, networks) if combined_networks.is_none() && networks.is_some() => {
                *combined_networks = networks;
            }
            _ => {}
        }

        match (&mut combined_file.volumes, file.volumes) {
            (Some(combined_volumes), Some(volumes)) => combined_volumes.extend(volumes),
            (combined_volumes, volumes) if combined_volumes.is_none() && volumes.is_some() => {
                *combined_volumes = volumes;
            }
            _ => {}
        }

        match (&mut combined_file.configs, file.configs) {
            (Some(combined_configs), Some(configs)) => combined_configs.extend(configs),
            (combined_configs, configs) if combined_configs.is_none() && configs.is_some() => {
                *combined_configs = configs;
            }
            _ => {}
        }

        match (&mut combined_file.secrets, file.secrets) {
            (Some(combined_secrets), Some(secrets)) => combined_secrets.extend(secrets),
            (combined_secrets, secrets) if combined_secrets.is_none() && secrets.is_some() => {
                *combined_secrets = secrets;
            }
            _ => {}
        }
    }

    Ok(combined_file)
}

#[cfg(test)]
mod tests {
    use serde_yaml::Value;

    use super::interpolate;

    #[test]
    fn simple_named() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(Value::String(String::from("$VAR")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn simple_named_missing() {
        let result = temp_env::with_var("VAR", None::<&str>, || {
            interpolate(Value::String(String::from("pre $VAR post")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("pre  post"))));
    }

    #[test]
    fn braced_named() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(Value::String(String::from("${VAR}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn braced_named_text() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(Value::String(String::from("pre ${VAR} post")))
        });

        assert_eq!(
            result.ok(),
            Some(Value::String(String::from("pre woop post")))
        );
    }

    #[test]
    fn default_named() {
        let result = temp_env::with_var("VAR", None::<&str>, || {
            interpolate(Value::String(String::from("${VAR-default}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("default"))));
    }

    #[test]
    fn no_default_named() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(Value::String(String::from("${VAR-default}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn default_pattern() {
        let result = temp_env::with_var("DEF", Some("woop"), || {
            interpolate(Value::String(String::from("${VAR-$DEF}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn default_named_no_empty() {
        let result = temp_env::with_var("VAR", Some(""), || {
            interpolate(Value::String(String::from("${VAR:-default}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("default"))));
    }

    #[test]
    fn no_default_named_no_empty() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(Value::String(String::from("${VAR:-default}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn default_pattern_no_empty() {
        let result = temp_env::with_vars(vec![("VAR", Some("")), ("DEF", Some("woop"))], || {
            interpolate(Value::String(String::from("${VAR:-$DEF}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::from("woop"))));
    }

    #[test]
    fn error_named() {
        let result = temp_env::with_var("VAR", None::<&str>, || {
            interpolate(Value::String(String::from("${VAR?msg}")))
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
            interpolate(Value::String(String::from("${VAR:?msg}")))
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
            interpolate(Value::String(String::from("${VAR?}")))
        });

        assert_eq!(
            result.err().map(|err| err.to_string()),
            Some(String::from("Required variable \"VAR\" is missing a value"))
        );
    }

    #[test]
    fn error_no_message_no_empty() {
        let result = temp_env::with_var("VAR", Some(""), || {
            interpolate(Value::String(String::from("${VAR:?}")))
        });

        assert_eq!(
            result.err().map(|err| err.to_string()),
            Some(String::from("Required variable \"VAR\" is missing a value"))
        );
    }

    #[test]
    fn replacement_named() {
        let result = temp_env::with_var("VAR", Some(""), || {
            interpolate(Value::String(String::from("${VAR+replacement}")))
        });

        assert_eq!(
            result.ok(),
            Some(Value::String(String::from("replacement")))
        );
    }

    #[test]
    fn no_replacement_named() {
        let result = temp_env::with_var("VAR", None::<&str>, || {
            interpolate(Value::String(String::from("${VAR+replacement}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::new())));
    }

    #[test]
    fn replacement_named_no_empty() {
        let result = temp_env::with_var("VAR", Some("woop"), || {
            interpolate(Value::String(String::from("${VAR:+replacement}")))
        });

        assert_eq!(
            result.ok(),
            Some(Value::String(String::from("replacement")))
        );
    }

    #[test]
    fn no_replacement_named_no_empty() {
        let result = temp_env::with_var("VAR", Some(""), || {
            interpolate(Value::String(String::from("${VAR:+replacement}")))
        });

        assert_eq!(result.ok(), Some(Value::String(String::new())));
    }
}
