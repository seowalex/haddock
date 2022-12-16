use anyhow::{anyhow, bail, Result};
use byte_unit::Byte;
use humantime::{format_duration, parse_duration};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use serde_with::{
    formats::SpaceSeparator, serde_as, serde_conv, skip_serializing_none, DisplayFromStr,
    DurationMicroSeconds, OneOrMany, PickFirst, StringWithSeparator,
};
use serde_yaml::Value;
use std::{
    convert::Infallible,
    hash::{Hash, Hasher},
    time::Duration,
};
use yansi::Paint;

use crate::utils::Merge;

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct Compose {
    pub(crate) version: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) services: IndexMap<String, Service>,
    pub(crate) networks: Option<IndexMap<String, Option<Network>>>,
    pub(crate) volumes: Option<IndexMap<String, Option<Volume>>>,
    pub(crate) configs: Option<IndexMap<String, Config>>,
    pub(crate) secrets: Option<IndexMap<String, Secret>>,
}

impl Compose {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.version.merge_one(other.version);
        self.name.merge_one(other.name);

        for (name, service) in other.services {
            self.services
                .entry(name)
                .and_modify(|combined_service| combined_service.merge(&service))
                .or_insert(service);
        }

        self.networks.merge(other.networks);
        self.volumes.merge(other.volumes);
        self.configs.merge(other.configs);
        self.secrets.merge(other.secrets);
    }
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Service {
    pub(crate) blkio_config: Option<BlkioConfig>,
    #[serde_as(as = "Option<PickFirst<(_, BuildConfigOrString)>>")]
    pub(crate) build: Option<BuildConfig>,
    pub(crate) cap_add: Option<Vec<String>>,
    pub(crate) cap_drop: Option<Vec<String>>,
    pub(crate) cgroup_parent: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<SpaceSeparator, String>)>>")]
    pub(crate) command: Option<Vec<String>>,
    #[serde_as(as = "Option<IndexSet<PickFirst<(_, FileReferenceOrString)>>>")]
    pub(crate) configs: Option<IndexSet<FileReference>>,
    pub(crate) container_name: Option<String>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_period: Option<Duration>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_quota: Option<Duration>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_rt_period: Option<Duration>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_rt_runtime: Option<Duration>,
    pub(crate) cpu_shares: Option<i64>,
    pub(crate) cpus: Option<f32>,
    pub(crate) cpuset: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, DependsOnVec)>>")]
    pub(crate) depends_on: Option<IndexMap<String, Dependency>>,
    pub(crate) deploy: Option<DeployConfig>,
    pub(crate) device_cgroup_rules: Option<Vec<String>>,
    #[serde_as(as = "Option<IndexSet<DeviceOrString>>")]
    pub(crate) devices: Option<IndexSet<Device>>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) dns: Option<Vec<String>>,
    pub(crate) dns_opt: Option<Vec<String>>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) dns_search: Option<Vec<String>>,
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<SpaceSeparator, String>)>>")]
    pub(crate) entrypoint: Option<Vec<String>>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) env_file: Option<Vec<String>>,
    #[serde_as(as = "Option<PickFirst<(_, MappingWithEqualsNull)>>")]
    pub(crate) environment: Option<IndexMap<String, Option<String>>>,
    pub(crate) expose: Option<Vec<String>>,
    pub(crate) extends: Option<Extends>,
    pub(crate) external_links: Option<Vec<String>>,
    pub(crate) extra_hosts: Option<Vec<String>>,
    pub(crate) group_add: Option<Vec<String>>,
    pub(crate) healthcheck: Option<Healthcheck>,
    pub(crate) hostname: Option<String>,
    pub(crate) image: Option<String>,
    pub(crate) init: Option<bool>,
    pub(crate) ipc: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, MappingWithEqualsEmpty)>>")]
    pub(crate) labels: Option<IndexMap<String, String>>,
    pub(crate) links: Option<Vec<String>>,
    pub(crate) logging: Option<Logging>,
    pub(crate) mac_address: Option<String>,
    pub(crate) mem_limit: Option<Byte>,
    pub(crate) mem_reservation: Option<Byte>,
    pub(crate) mem_swappiness: Option<i64>,
    pub(crate) memswap_limit: Option<SwapLimit>,
    #[serde_as(as = "Option<PickFirst<(_, NetworksVec)>>")]
    pub(crate) networks: Option<IndexMap<String, Option<ServiceNetwork>>>,
    pub(crate) network_mode: Option<String>,
    pub(crate) oom_kill_disable: Option<bool>,
    pub(crate) oom_score_adj: Option<i64>,
    pub(crate) pid: Option<String>,
    pub(crate) pids_limit: Option<i64>,
    pub(crate) platform: Option<String>,
    #[serde_as(as = "Option<Vec<PickFirst<(_, PortOrString, PortOrU32)>>>")]
    pub(crate) ports: Option<Vec<Port>>,
    pub(crate) privileged: Option<bool>,
    pub(crate) profiles: Option<Vec<String>>,
    pub(crate) pull_policy: Option<PullPolicy>,
    pub(crate) read_only: Option<bool>,
    pub(crate) restart: Option<RestartPolicy>,
    pub(crate) runtime: Option<String>,
    #[serde_as(as = "Option<IndexSet<PickFirst<(_, FileReferenceOrString)>>>")]
    pub(crate) secrets: Option<IndexSet<FileReference>>,
    pub(crate) security_opt: Option<Vec<String>>,
    pub(crate) shm_size: Option<Byte>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) stop_grace_period: Option<Duration>,
    pub(crate) stop_signal: Option<String>,
    pub(crate) storage_opt: Option<IndexMap<String, String>>,
    #[serde_as(as = "Option<PickFirst<(_, MappingWithEqualsNoNull)>>")]
    pub(crate) sysctls: Option<IndexMap<String, String>>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) tmpfs: Option<Vec<String>>,
    pub(crate) tty: Option<bool>,
    pub(crate) ulimits: Option<IndexMap<String, ResourceLimit>>,
    pub(crate) user: Option<String>,
    pub(crate) userns_mode: Option<String>,
    #[serde_as(as = "Option<IndexSet<PickFirst<(_, ServiceVolumeOrString)>>>")]
    pub(crate) volumes: Option<IndexSet<ServiceVolume>>,
    pub(crate) volumes_from: Option<Vec<String>>,
    pub(crate) working_dir: Option<String>,
}

fn merge(base: &mut Value, other: Value) {
    match (base, other) {
        (base @ Value::Mapping(_), Value::Mapping(other)) => {
            let a = base.as_mapping_mut().unwrap();

            for (key, other_value) in other {
                a.entry(key.clone())
                    .and_modify(|value| match key.as_str().unwrap() {
                        "command" | "entrypoint" => *value = other_value.clone(),
                        _ => merge(value, other_value.clone()),
                    })
                    .or_insert(other_value);
            }
        }
        (Value::Sequence(base), Value::Sequence(other)) => {
            base.extend(other);
        }
        (base, other) => *base = other,
    }
}

impl Service {
    pub(crate) fn merge(&mut self, other: &Self) {
        let mut value = serde_yaml::to_value(&self).unwrap();
        merge(&mut value, serde_yaml::to_value(other).unwrap());

        *self = serde_yaml::from_value(value).unwrap();
    }
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct BlkioConfig {
    pub(crate) weight: Option<u16>,
    pub(crate) weight_device: Option<Vec<WeightDevice>>,
    pub(crate) device_read_bps: Option<Vec<ThrottleDevice>>,
    pub(crate) device_write_bps: Option<Vec<ThrottleDevice>>,
    pub(crate) device_read_iops: Option<Vec<ThrottleDevice>>,
    pub(crate) device_write_iops: Option<Vec<ThrottleDevice>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct WeightDevice {
    pub(crate) path: String,
    pub(crate) weight: u16,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ThrottleDevice {
    pub(crate) path: String,
    pub(crate) rate: Byte,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct BuildConfig {
    pub(crate) context: String,
    pub(crate) dockerfile: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, MappingWithEqualsNull)>>")]
    pub(crate) args: Option<IndexMap<String, Option<String>>>,
    pub(crate) ssh: Option<Vec<String>>,
    pub(crate) cache_from: Option<Vec<String>>,
    pub(crate) cache_to: Option<Vec<String>>,
    pub(crate) extra_hosts: Option<Vec<String>>,
    pub(crate) isolation: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, MappingWithEqualsEmpty)>>")]
    pub(crate) labels: Option<IndexMap<String, String>>,
    pub(crate) no_cache: Option<bool>,
    pub(crate) pull: Option<bool>,
    pub(crate) shm_size: Option<Byte>,
    pub(crate) target: Option<String>,
    #[serde_as(as = "Option<IndexSet<PickFirst<(_, FileReferenceOrString)>>>")]
    pub(crate) secrets: Option<IndexSet<FileReference>>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) platforms: Option<Vec<String>>,
}

serde_conv!(
    BuildConfigOrString,
    BuildConfig,
    |build: &BuildConfig| { build.context.clone() },
    |context| -> std::result::Result<_, Infallible> {
        Ok(BuildConfig {
            context,
            ..BuildConfig::default()
        })
    }
);

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct FileReference {
    pub(crate) source: String,
    pub(crate) target: Option<String>,
    pub(crate) uid: Option<String>,
    pub(crate) gid: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, DisplayFromStr)>>")]
    pub(crate) mode: Option<u32>,
}

impl PartialEq for FileReference {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for FileReference {}

impl Hash for FileReference {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
    }
}

serde_conv!(
    FileReferenceOrString,
    FileReference,
    |file_reference: &FileReference| { file_reference.source.clone() },
    |source| -> std::result::Result<_, Infallible> {
        Ok(FileReference {
            source,
            ..Default::default()
        })
    }
);

serde_conv!(
    DurationWithSuffix,
    Duration,
    |duration: &Duration| format_duration(*duration).to_string(),
    |duration: String| parse_duration(&duration)
);

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Dependency {
    pub(crate) condition: Condition,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum Condition {
    #[serde(rename = "service_started")]
    Started,
    #[serde(rename = "service_healthy")]
    Healthy,
    #[serde(rename = "service_completed_successfully")]
    CompletedSuccessfully,
}

serde_conv!(
    DependsOnVec,
    IndexMap<String, Dependency>,
    |dependencies: &IndexMap<String, Dependency>| dependencies.keys().cloned().collect::<Vec<_>>(),
    |dependencies: Vec<String>| -> std::result::Result<_, Infallible> {
        Ok(dependencies.into_iter().map(
            |dependency| {
                (
                    dependency,
                    Dependency {
                        condition: Condition::Started,
                    },
                )
            },
        ).collect::<IndexMap<_, _>>())
    }
);

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct DeployConfig {
    pub(crate) resources: Option<Resources>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Resources {
    pub(crate) limits: Option<Resource>,
    pub(crate) reservations: Option<Resource>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Resource {
    #[serde_as(as = "Option<PickFirst<(_, DisplayFromStr)>>")]
    pub(crate) cpus: Option<f32>,
    pub(crate) memory: Option<Byte>,
    pub(crate) pids: Option<i64>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Device {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) permissions: Option<String>,
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target
    }
}

impl Eq for Device {}

impl Hash for Device {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.target.hash(state);
    }
}

serde_conv!(
    DeviceOrString,
    Device,
    |device: &Device| {
        if let Some(permissions) = &device.permissions {
            format!("{}:{}:{permissions}", device.source, device.target)
        } else {
            format!("{}:{}", device.source, device.target)
        }
    },
    |device: String| -> std::result::Result<_, Infallible> {
        let mut parts = device.split(':');

        Ok(Device {
            source: parts.next().unwrap().to_string(),
            target: parts.next().unwrap().to_string(),
            permissions: parts.next().map(ToString::to_string),
        })
    }
);

serde_conv!(
    MappingWithEqualsNull,
    IndexMap<String, Option<String>>,
    |variables: &IndexMap<String, Option<String>>| {
        variables
            .iter()
            .map(|(key, value)| match value {
                Some(value) => format!("{key}={value}"),
                None => key.clone(),
            })
            .collect::<Vec<_>>()
    },
    |variables: Vec<String>| -> std::result::Result<_, Infallible> {
        Ok(variables.into_iter().map(|variable| {
            let mut parts = variable.split('=');
            (
                parts.next().unwrap().to_string(),
                parts.next().map(ToString::to_string),
            )
        }).collect::<IndexMap<_, _>>())
    }
);

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Extends {
    pub(crate) service: String,
    pub(crate) file: Option<String>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Healthcheck {
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<SpaceSeparator, String>)>>")]
    pub(crate) test: Option<Vec<String>>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) interval: Option<Duration>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) timeout: Option<Duration>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) start_period: Option<Duration>,
    pub(crate) retries: Option<u64>,
    pub(crate) disable: Option<bool>,
}

serde_conv!(
    MappingWithEqualsEmpty,
    IndexMap<String, String>,
    |variables: &IndexMap<String, String>| {
        variables
            .iter()
            .map(|(key, value)| {
                if value.is_empty() {
                    key.clone()
                } else {
                    format!("{key}={value}")
                }
            })
            .collect::<Vec<_>>()
    },
    |variables: Vec<String>| -> std::result::Result<_, Infallible> {
        Ok(variables.into_iter().map(|variable| {
            let mut parts = variable.split('=');
            (
                parts.next().unwrap().to_string(),
                parts.next().map(ToString::to_string).unwrap_or_default(),
            )
        }).collect::<IndexMap<_, _>>())
    }
);

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Logging {
    pub(crate) driver: Option<String>,
    pub(crate) options: Option<IndexMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum SwapLimit {
    Limited(Byte),
    Unlimited(i8),
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ServiceNetwork {
    pub(crate) aliases: Option<Vec<String>>,
    pub(crate) ipv4_address: Option<String>,
    pub(crate) ipv6_address: Option<String>,
    pub(crate) link_local_ips: Option<Vec<String>>,
    pub(crate) priority: Option<i32>,
}

serde_conv!(
    NetworksVec,
    IndexMap<String, Option<ServiceNetwork>>,
    |networks: &IndexMap<String, Option<ServiceNetwork>>| networks.keys().cloned().collect::<Vec<_>>(),
    |networks: Vec<String>| -> std::result::Result<_, Infallible> {
        Ok(networks.into_iter().map(|network| (network, None)).collect::<IndexMap<_, _>>())
    }
);

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct Port {
    #[serde_as(as = "PickFirst<(_, StringOrU16)>")]
    pub(crate) target: String,
    #[serde_as(as = "Option<PickFirst<(_, StringOrU16)>>")]
    pub(crate) published: Option<String>,
    pub(crate) host_ip: Option<String>,
    pub(crate) protocol: Option<String>,
}

serde_conv!(
    PortOrString,
    Port,
    |port: &Port| {
        let mut string = port.target.clone();

        match (&port.published, &port.host_ip) {
            (None, None) => {}
            (published, host_ip) => {
                string = format!("{}:{string}", published.clone().unwrap_or_default());

                if let Some(host_ip) = host_ip {
                    string = format!("{host_ip}:{string}");
                }
            }
        }

        if let Some(protocol) = &port.protocol {
            string = format!("{string}/{protocol}");
        }

        string
    },
    |port: String| -> std::result::Result<_, Infallible> {
        let mut parts = port.split(':').rev();
        let container_port = parts.next().unwrap();
        let mut container_parts = container_port.split('/');
        let target = container_parts.next().unwrap().to_string();

        Ok(Port {
            target,
            published: parts.next().and_then(|part| {
                if part.is_empty() {
                    None
                } else {
                    Some(part.to_string())
                }
            }),
            host_ip: parts.next().map(ToString::to_string),
            protocol: container_parts.next().map(ToString::to_string),
        })
    }
);

serde_conv!(
    PortOrU32,
    Port,
    |port: &Port| port.target.parse::<u32>().unwrap(),
    |target: u32| -> std::result::Result<_, Infallible> {
        Ok(Port {
            target: target.to_string(),
            ..Default::default()
        })
    }
);

serde_conv!(
    StringOrU16,
    String,
    |port: &String| port.parse::<u16>().unwrap(),
    |port: u16| -> std::result::Result<_, Infallible> { Ok(port.to_string()) }
);

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PullPolicy {
    Always,
    Never,
    Missing,
    Build,
    Newer,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RestartPolicy {
    No,
    Always,
    OnFailure,
    UnlessStopped,
}

serde_conv!(
    MappingWithEqualsNoNull,
    IndexMap<String, String>,
    |variables: &IndexMap<String, String>| {
        variables
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
    },
    |variables: Vec<String>| -> Result<_> {
        let variables = variables.into_iter().map(|variable| -> Result<_> {
            let mut parts = variable.split('=');
            let key = parts.next().unwrap().to_string();
            let value = parts.next().map(ToString::to_string).ok_or_else(|| anyhow!("{key}: value not defined"))?;

            Ok((key, value))
        }).collect::<Result<Vec<_>, _>>()?;

        Ok(variables.into_iter().collect::<IndexMap<_, _>>())
    }
);

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum ResourceLimit {
    Single(i32),
    Double { soft: i32, hard: i32 },
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ServiceVolume {
    pub(crate) r#type: ServiceVolumeType,
    pub(crate) source: Option<String>,
    pub(crate) target: String,
    pub(crate) read_only: Option<bool>,
    pub(crate) bind: Option<ServiceVolumeBind>,
    pub(crate) volume: Option<ServiceVolumeVolume>,
    pub(crate) tmpfs: Option<ServiceVolumeTmpfs>,
}

impl PartialEq for ServiceVolume {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target
    }
}

impl Eq for ServiceVolume {}

impl Hash for ServiceVolume {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.target.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceVolumeType {
    Volume,
    Bind,
    Tmpfs,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct ServiceVolumeBind {
    pub(crate) propagation: Option<String>,
    pub(crate) create_host_path: Option<bool>,
    pub(crate) selinux: Option<String>,
}

impl ServiceVolumeBind {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ServiceVolumeVolume {
    pub(crate) nocopy: Option<bool>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ServiceVolumeTmpfs {
    pub(crate) size: Option<Byte>,
    #[serde_as(as = "Option<PickFirst<(_, DisplayFromStr)>>")]
    pub(crate) mode: Option<u32>,
}

serde_conv!(
    ServiceVolumeOrString,
    ServiceVolume,
    |_| {},
    |mount: String| -> Result<_> {
        let mut r#type = ServiceVolumeType::Volume;
        let mut source = None;
        let target;
        let mut read_only = None;
        let mut bind = None;
        let mut volume = None;
        let mut options = "";
        let parts = mount.split(':').collect::<Vec<_>>();

        match parts[..] {
            [dst] => {
                target = dst.to_string();
            }
            [src, dst] if dst.starts_with('/') => {
                if src.starts_with('/') || src.starts_with('.') {
                    r#type = ServiceVolumeType::Bind;
                }

                source = Some(src.to_string());
                target = dst.to_string();
            }
            [dst, opts] => {
                target = dst.to_string();
                options = opts;
            }
            [src, dst, opts] => {
                if src.starts_with('/') || src.starts_with('.') {
                    r#type = ServiceVolumeType::Bind;
                }

                source = Some(src.to_string());
                target = dst.to_string();
                options = opts;
            }
            _ => {
                bail!("{mount}: too many colons");
            }
        }

        let options = options.split(',');
        let mut unused = vec![];

        for option in options {
            match option {
                "rw" | "ro" => {
                    read_only = Some(option == "ro");
                }
                "shared" | "rshared" | "slave" | "rslave" | "private" | "rprivate"
                | "unbindable" | "runbindable" => {
                    bind.get_or_insert(ServiceVolumeBind::new()).propagation =
                        Some(option.to_string());
                }
                "z" | "Z" => {
                    bind.get_or_insert(ServiceVolumeBind::new()).selinux = Some(option.to_string());
                }
                "copy" | "nocopy" => {
                    volume = Some(ServiceVolumeVolume {
                        nocopy: Some(option == "nocopy"),
                    });
                }
                "" => {}
                _ => {
                    unused.push(option);
                }
            }
        }

        if !unused.is_empty() {
            eprintln!(
                "{} Unsupported/unknown mount options: {}",
                Paint::yellow("Warning:").bold(),
                unused.join(", ")
            );
        }

        Ok(ServiceVolume {
            r#type,
            source,
            target,
            read_only,
            bind,
            volume,
            tmpfs: None,
        })
    }
);

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Network {
    pub(crate) driver: Option<String>,
    pub(crate) driver_opts: Option<IndexMap<String, String>>,
    pub(crate) enable_ipv6: Option<bool>,
    pub(crate) ipam: Option<IpamConfig>,
    pub(crate) internal: Option<bool>,
    #[serde_as(as = "Option<PickFirst<(_, MappingWithEqualsEmpty)>>")]
    pub(crate) labels: Option<IndexMap<String, String>>,
    pub(crate) external: Option<bool>,
    pub(crate) name: Option<String>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct IpamConfig {
    pub(crate) driver: Option<String>,
    pub(crate) config: Option<Vec<IpamPool>>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct IpamPool {
    pub(crate) subnet: Option<String>,
    pub(crate) ip_range: Option<String>,
    pub(crate) gateway: Option<String>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Volume {
    pub(crate) driver: Option<String>,
    pub(crate) driver_opts: Option<IndexMap<String, String>>,
    pub(crate) external: Option<bool>,
    #[serde_as(as = "Option<PickFirst<(_, MappingWithEqualsEmpty)>>")]
    pub(crate) labels: Option<IndexMap<String, String>>,
    pub(crate) name: Option<String>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Config {
    pub(crate) file: Option<String>,
    pub(crate) external: Option<bool>,
    pub(crate) name: Option<String>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Secret {
    pub(crate) file: Option<String>,
    pub(crate) environment: Option<String>,
    pub(crate) external: Option<bool>,
    pub(crate) name: Option<String>,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use std::fs;
    use test_generator::test_resources;

    use super::*;

    #[test_resources("tests/fixtures/**/*.y*ml")]
    fn serde(resource: &str) {
        let contents = fs::read_to_string(resource).unwrap();

        assert!(serde_yaml::from_str::<Compose>(&contents).is_ok());
    }

    #[test]
    fn merge() {
        let base = fs::read_to_string("tests/fixtures/override/compose.yaml").unwrap();
        let other = fs::read_to_string("tests/fixtures/override/compose.override.yaml").unwrap();

        let mut result = serde_yaml::from_str::<Compose>(&base).unwrap();
        result.merge(serde_yaml::from_str(&other).unwrap());

        let expected = fs::read_to_string("tests/fixtures/override/compose.expected.yaml").unwrap();

        assert_eq!(
            format!("{result:#?}"),
            format!("{:#?}", serde_yaml::from_str::<Compose>(&expected).unwrap())
        );
    }
}
