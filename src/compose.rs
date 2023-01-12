mod parser;
pub(crate) mod types;

use std::{
    env::{self, VarError},
    fs,
    io::{self, Read},
};

use anyhow::{anyhow, bail, Context, Error, Result};
use indexmap::IndexSet;
use itertools::Itertools;
use path_absolutize::Absolutize;
use petgraph::{algo::tarjan_scc, graphmap::DiGraphMap};
use serde_yaml::Value;

use self::{
    parser::{State, Token, Var},
    types::{Compose, Condition, ServiceVolumeType},
};
use crate::{
    config::Config,
    utils::{regex, STYLED_WARNING},
};

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
                            Err(VarError::NotPresent)
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
                            Err(VarError::NotPresent)
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
                            Err(VarError::NotPresent)
                        } else {
                            Ok(var)
                        }
                    }),
                }
                .map_or_else(|_| Ok(String::new()), |_| evaluate(tokens)),
                None => Ok(env::var(&name).unwrap_or_else(|_| {
                    eprintln!(
                        "{} The \"{name}\" variable is not set, defaulting to a blank string",
                        *STYLED_WARNING
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
        values.iter().map(interpolate).collect()
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

pub(crate) fn parse(config: &Config, no_interpolate: bool) -> Result<Compose> {
    let contents = config
        .files
        .iter()
        .map(|path| {
            if path.as_os_str() == "-" {
                let mut content = String::new();
                let mut stdin = io::stdin();

                stdin.read_to_string(&mut content)?;

                Ok((path, content))
            } else {
                fs::read_to_string(path)
                    .with_context(|| format!("{} not found", path.display()))
                    .map(|content| (path, content))
            }
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
                                            .unwrap_or_default(),
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
            if no_interpolate {
                content
            } else {
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
            }
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
                .with_context(|| {
                    format!(
                        "{} does not follow the Compose specification",
                        path.display()
                    )
                })
                .map(|file: Compose| (path, file, unused))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut combined_file = Compose::new();

    for (path, file, unused) in files {
        if !unused.is_empty() {
            eprintln!(
                "{} Unsupported/unknown properties in {}: {}",
                *STYLED_WARNING,
                path.display(),
                unused.into_iter().join(", ")
            );
        }

        combined_file.merge(file);
    }

    combined_file.services.retain(|_, service| {
        if service.profiles.is_empty() {
            return true;
        }

        for profile in &service.profiles {
            if config.profiles.contains(profile) {
                return true;
            }
        }

        false
    });

    let mut all_networks = IndexSet::new();
    let mut all_volumes = IndexSet::new();
    let mut all_secrets = IndexSet::new();

    for service in combined_file.services.values_mut() {
        if let Some(build) = &mut service.build {
            build.dockerfile = build
                .dockerfile
                .absolutize_from(&build.context)?
                .to_path_buf();
        }

        all_networks.extend(service.networks.keys());
        all_volumes.extend(
            service
                .volumes
                .iter()
                .filter_map(|volume| match &volume.r#type {
                    ServiceVolumeType::Volume(source) => source.as_ref(),
                    _ => None,
                }),
        );
        all_secrets.extend(service.secrets.iter().map(|secret| &secret.source));
    }

    combined_file
        .networks
        .entry(String::from("default"))
        .or_default();
    combined_file
        .networks
        .retain(|network, _| all_networks.contains(network));

    for (name, network) in &mut combined_file.networks {
        network.name.get_or_insert_with(|| {
            match (network.external.unwrap_or_default(), &combined_file.name) {
                (false, Some(project_name)) => format!("{project_name}_{name}"),
                _ => name.clone(),
            }
        });
    }

    combined_file
        .volumes
        .retain(|volume, _| all_volumes.contains(volume));

    for (name, volume) in &mut combined_file.volumes {
        volume.name.get_or_insert_with(|| {
            match (volume.external.unwrap_or_default(), &combined_file.name) {
                (false, Some(project_name)) => format!("{project_name}_{name}"),
                _ => name.clone(),
            }
        });
    }

    combined_file
        .secrets
        .retain(|secrets, _| all_secrets.contains(secrets));

    for (name, secret) in &mut combined_file.secrets {
        secret.name.get_or_insert_with(|| {
            match (secret.external.unwrap_or_default(), &combined_file.name) {
                (false, Some(project_name)) => format!("{project_name}_{name}"),
                _ => name.clone(),
            }
        });
    }

    for (name, service) in &combined_file.services {
        if service.scale.is_some() {
            eprintln!(
                "{} `scale` is deprecated, use the `deploy.replicas` element instead",
                *STYLED_WARNING
            );
        }

        if service.mem_limit.is_some() {
            eprintln!(
                "{} `mem_limit` is deprecated, use the `deploy.limits.memory` element instead",
                *STYLED_WARNING
            );
        }

        if service.cpus.is_some() {
            eprintln!(
                "{} `cpus` is deprecated, use the `deploy.reservations.cpus` element instead",
                *STYLED_WARNING
            );
        }

        if service.mem_reservation.is_some() {
            eprintln!(
                "{} `mem_reservation` is deprecated, use the `deploy.reservations.memory` element instead",
                *STYLED_WARNING
            );
        }

        if service.pids_limit.is_some() {
            eprintln!(
                "{} `pids_limit` is deprecated, use the `deploy.reservations.pids` element instead",
                *STYLED_WARNING
            );
        }

        if service
            .depends_on
            .values()
            .any(|dependency| dependency.condition != Condition::Started)
        {
            eprintln!(
                "{} \"service_healthy\" and \"service_completed_successfully\" are unsupported and will degrade to \"service_started\"",
                *STYLED_WARNING
            );
        }

        if service.build.is_none() && service.image.is_none() {
            bail!("Service \"{name}\" has neither an image nor a build context specified");
        }

        if service.network_mode.as_deref().unwrap_or_default() == "host"
            && !service.ports.is_empty()
        {
            bail!("Service \"{name}\" cannot have port mappings due to host network mode");
        }

        if service.container_name.is_some()
            && service
                .deploy
                .as_ref()
                .and_then(|deploy| deploy.replicas)
                .or(service.scale)
                .unwrap_or(1)
                > 1
        {
            bail!(
                "Service \"{name}\" cannot scale beyond one container as it has a container name"
            );
        }

        for label in service.labels.keys() {
            if label.starts_with("io.podman.compose") {
                bail!("Service \"name\" cannot have labels starting with \"io.podman.compose\"");
            }
        }

        if let Some(build) = &service.build {
            for label in build.labels.keys() {
                if label.starts_with("io.podman.compose") {
                    bail!(
                        "Service \"name\" cannot have labels starting with \"io.podman.compose\""
                    );
                }
            }
        }

        for dependency in service.depends_on.keys().chain(service.links.keys()) {
            if !combined_file.services.contains_key(dependency) {
                bail!("Service \"{name}\" depends on undefined service \"{dependency}\"");
            }
        }

        for network in service.networks.keys() {
            if !combined_file.networks.contains_key(network) {
                bail!("Service \"{name}\" refers to undefined network \"{network}\"",);
            }
        }

        for volume in service
            .volumes
            .iter()
            .filter_map(|volume| match &volume.r#type {
                ServiceVolumeType::Volume(source) => source.as_ref(),
                _ => None,
            })
        {
            if !combined_file.volumes.contains_key(volume) {
                bail!("Service \"{name}\" refers to undefined volume \"{volume}\"");
            }
        }

        for secret in &service.secrets {
            if !combined_file.secrets.contains_key(&secret.source) {
                bail!(
                    "Service \"{name}\" refers to undefined secret \"{}\"",
                    secret.source
                );
            }
        }
    }

    for (name, network) in &combined_file.networks {
        if network.external.unwrap_or_default()
            && (network.driver.is_some()
                || !network.driver_opts.is_empty()
                || network.enable_ipv6.is_some()
                || network.ipam.is_some()
                || network.internal.is_some()
                || !network.labels.is_empty())
        {
            bail!("Conflicting parameters specified for network \"{name}\"");
        }

        for label in network.labels.keys() {
            if label.starts_with("io.podman.compose") {
                bail!("Network \"name\" cannot have labels starting with \"io.podman.compose\"");
            }
        }
    }

    for (name, volume) in &combined_file.volumes {
        if volume.external.unwrap_or_default()
            && (volume.driver.is_some()
                || !volume.driver_opts.is_empty()
                || !volume.labels.is_empty())
        {
            bail!("Conflicting parameters specified for volume \"{name}\"");
        }

        for label in volume.labels.keys() {
            if label.starts_with("io.podman.compose") {
                bail!("Volume \"name\" cannot have labels starting with \"io.podman.compose\"");
            }
        }
    }

    for (name, secret) in &combined_file.secrets {
        if secret.external.unwrap_or_default()
            && (secret.file.is_some() || secret.environment.is_some())
        {
            bail!("Conflicting parameters specified for secret \"{name}\"");
        }
    }

    let dependencies = combined_file
        .services
        .iter()
        .flat_map(|(to, service)| {
            service
                .depends_on
                .keys()
                .chain(service.links.keys())
                .map(move |from| (from, to, ()))
        })
        .collect::<DiGraphMap<_, _>>();
    let cycles = tarjan_scc(&dependencies)
        .into_iter()
        .filter(|component| component.len() > 1)
        .collect::<Vec<_>>();

    if !cycles.is_empty() {
        bail!(
            "Cycles found: {}",
            cycles
                .into_iter()
                .map(|component| format!("{} -> {}", component.iter().join(" -> "), component[0]))
                .join(", ")
        );
    }

    Ok(combined_file)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;
    use serde_yaml::Value;
    use test_generator::test_resources;
    use tokio_test::assert_ok;

    use super::*;

    #[test_resources("tests/fixtures/**/*.y*ml")]
    fn parse(resource: &str) {
        let config = Config {
            files: vec![PathBuf::from(resource)],
            ..Config::default()
        };

        assert_ok!(super::parse(&config, false));
    }

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
