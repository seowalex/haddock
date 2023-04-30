use std::{
    convert::Infallible,
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{anyhow, bail, Error, Result};
use byte_unit::Byte;
use heck::AsKebabCase;
use humantime::{format_duration, parse_duration};
use indexmap::{indexmap, IndexMap, IndexSet};
use path_absolutize::Absolutize;
use serde::{Deserialize, Serialize};
use serde_with::{
    formats::PreferMany, serde_as, serde_conv, skip_serializing_none, DefaultOnNull,
    DisplayFromStr, DurationMicroSeconds, OneOrMany, PickFirst, SetLastValueWins,
};
use serde_yaml::Value;

use crate::utils::{DisplayFromAny, STYLED_WARNING};

#[skip_serializing_none]
#[serde_as]
#[serde_with::apply(
    IndexMap => #[serde(skip_serializing_if = "IndexMap::is_empty", default)]
)]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct Compose {
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
    #[serde_with(skip_apply)]
    #[serde(default)]
    pub(crate) services: IndexMap<String, Service>,
    #[serde_as(as = "IndexMap<_, DefaultOnNull>")]
    pub(crate) networks: IndexMap<String, Network>,
    #[serde_as(as = "IndexMap<_, DefaultOnNull>")]
    pub(crate) volumes: IndexMap<String, Volume>,
    pub(crate) secrets: IndexMap<String, Secret>,
}

impl Compose {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn merge(&mut self, other: Self) {
        if other.version.is_some() {
            self.version = other.version;
        }

        if other.name.is_some() {
            self.name = other.name;
        }

        for (name, service) in other.services {
            self.services
                .entry(name)
                .and_modify(|combined_service| combined_service.merge(&service))
                .or_insert(service);
        }

        self.networks = other.networks;
        self.volumes = other.volumes;
        self.secrets = other.secrets;
    }
}

#[skip_serializing_none]
#[serde_as]
#[serde_with::apply(
    IndexMap => #[serde(skip_serializing_if = "IndexMap::is_empty", default)],
    IndexSet => #[serde(skip_serializing_if = "IndexSet::is_empty", default)],
    Vec => #[serde(skip_serializing_if = "Vec::is_empty", default)]
)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Service {
    pub(crate) blkio_config: Option<BlkioConfig>,
    pub(crate) cap_add: Vec<String>,
    pub(crate) cap_drop: Vec<String>,
    pub(crate) cgroup: Option<String>,
    pub(crate) cgroup_parent: Option<String>,
    #[serde_as(as = "PickFirst<(_, CommandOrString)>")]
    pub(crate) command: Vec<String>,
    pub(crate) container_name: Option<String>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_period: Option<Duration>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_quota: Option<Duration>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_rt_period: Option<Duration>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_rt_runtime: Option<Duration>,
    pub(crate) cpu_shares: Option<i32>,
    pub(crate) cpus: Option<f64>,
    pub(crate) cpuset: Option<String>,
    #[serde_as(as = "PickFirst<(_, IndexMap<DisplayFromAny, _>, DependsOnVec)>")]
    pub(crate) depends_on: IndexMap<String, Dependency>,
    pub(crate) deploy: Option<DeployConfig>,
    pub(crate) device_cgroup_rules: Vec<String>,
    #[serde_as(as = "SetLastValueWins<DeviceOrString>")]
    pub(crate) devices: IndexSet<Device>,
    #[serde_as(as = "OneOrMany<_, PreferMany>")]
    pub(crate) dns: Vec<String>,
    pub(crate) dns_opt: Vec<String>,
    #[serde_as(as = "OneOrMany<_, PreferMany>")]
    pub(crate) dns_search: Vec<String>,
    #[serde_as(as = "PickFirst<(_, CommandOrString)>")]
    pub(crate) entrypoint: Vec<String>,
    #[serde_as(as = "OneOrMany<AbsPathBuf, PreferMany>")]
    pub(crate) env_file: Vec<PathBuf>,
    #[serde_as(
        as = "PickFirst<(_, IndexMap<DisplayFromAny, Option<DisplayFromAny>>, MappingWithEqualsNull)>"
    )]
    pub(crate) environment: IndexMap<String, Option<String>>,
    pub(crate) expose: Vec<String>,
    #[serde_as(
        as = "PickFirst<(_, IndexMap<DisplayFromAny, DisplayFromAny>, MappingWithColonEmpty)>"
    )]
    pub(crate) extra_hosts: IndexMap<String, String>,
    pub(crate) group_add: Vec<String>,
    pub(crate) healthcheck: Option<Healthcheck>,
    pub(crate) hostname: Option<String>,
    pub(crate) image: Option<String>,
    pub(crate) init: Option<bool>,
    pub(crate) ipc: Option<String>,
    #[serde_as(
        as = "PickFirst<(_, IndexMap<DisplayFromAny, DisplayFromAny>, MappingWithEqualsEmpty)>"
    )]
    pub(crate) labels: IndexMap<String, String>,
    pub(crate) logging: Option<Logging>,
    pub(crate) mac_address: Option<String>,
    pub(crate) mem_limit: Option<Byte>,
    pub(crate) mem_reservation: Option<Byte>,
    pub(crate) mem_swappiness: Option<i32>,
    pub(crate) memswap_limit: Option<SwapLimit>,
    #[serde_as(as = "PickFirst<(_, IndexMap<DisplayFromAny, _>, NetworksVec)>")]
    #[serde_with(skip_apply)]
    #[serde(default = "default_service_networks")]
    pub(crate) networks: IndexMap<String, Option<ServiceNetwork>>,
    pub(crate) network_mode: Option<String>,
    pub(crate) oom_kill_disable: Option<bool>,
    pub(crate) oom_score_adj: Option<i32>,
    pub(crate) pid: Option<String>,
    pub(crate) pids_limit: Option<i32>,
    pub(crate) platform: Option<String>,
    #[serde_as(as = "Vec<PickFirst<(_, PortOrString, PortOrU16)>>")]
    pub(crate) ports: Vec<Port>,
    pub(crate) privileged: Option<bool>,
    pub(crate) profiles: Vec<String>,
    pub(crate) pull_policy: Option<PullPolicy>,
    pub(crate) read_only: Option<bool>,
    pub(crate) restart: Option<RestartPolicy>,
    pub(crate) runtime: Option<String>,
    pub(crate) scale: Option<u32>,
    #[serde_as(as = "SetLastValueWins<PickFirst<(_, FileReferenceOrString)>>")]
    pub(crate) secrets: IndexSet<FileReference>,
    #[serde_as(as = "SecurityOptVec")]
    pub(crate) security_opt: Vec<(String, Option<String>)>,
    pub(crate) shm_size: Option<Byte>,
    pub(crate) stdin_open: Option<bool>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) stop_grace_period: Option<Duration>,
    pub(crate) stop_signal: Option<String>,
    pub(crate) storage_opt: IndexMap<String, String>,
    #[serde_as(
        as = "PickFirst<(_, IndexMap<DisplayFromAny, DisplayFromAny>, MappingWithEqualsNoNull)>"
    )]
    pub(crate) sysctls: IndexMap<String, String>,
    #[serde_as(as = "OneOrMany<_, PreferMany>")]
    pub(crate) tmpfs: Vec<PathBuf>,
    pub(crate) tty: Option<bool>,
    pub(crate) ulimits: IndexMap<String, ResourceLimit>,
    pub(crate) user: Option<String>,
    pub(crate) userns_mode: Option<String>,
    pub(crate) uts: Option<String>,
    #[serde_as(as = "SetLastValueWins<PickFirst<(_, ServiceVolumeOrString)>>")]
    pub(crate) volumes: IndexSet<ServiceVolume>,
    pub(crate) volumes_from: Vec<String>,
    pub(crate) working_dir: Option<PathBuf>,
}

fn default_service_networks() -> IndexMap<String, Option<ServiceNetwork>> {
    indexmap! {
        String::from("default") => None
    }
}

fn merge(base: &mut Value, other: Value) {
    match (base, other) {
        (base @ Value::Mapping(_), Value::Mapping(other)) => {
            let base = base.as_mapping_mut().unwrap();

            for (key, other_value) in other {
                base.entry(key.clone())
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

    pub(crate) fn to_args(&self) -> (Vec<String>, Vec<String>) {
        let mut global_args = Vec::new();
        let mut args = Vec::new();

        if let Some(blkio_config) = &self.blkio_config {
            if let Some(weight) = blkio_config.weight {
                args.extend([String::from("--blkio-weight"), weight.to_string()]);
            }

            for weight_device in &blkio_config.weight_device {
                args.extend([
                    String::from("--blkio-weight-device"),
                    weight_device.to_string(),
                ]);
            }

            for device_read_bps in &blkio_config.device_read_bps {
                args.extend([
                    String::from("--device-read-bps"),
                    device_read_bps.to_string(),
                ]);
            }

            for device_write_bps in &blkio_config.device_write_bps {
                args.extend([
                    String::from("--device-write-bps"),
                    device_write_bps.to_string(),
                ]);
            }

            for device_read_iops in &blkio_config.device_read_iops {
                args.extend([
                    String::from("--device-read-iops"),
                    device_read_iops.to_string(),
                ]);
            }

            for device_write_iops in &blkio_config.device_write_iops {
                args.extend([
                    String::from("--device-write-iops"),
                    device_write_iops.to_string(),
                ]);
            }
        }

        for cap_add in self.cap_add.iter().cloned() {
            args.extend([String::from("--cap-add"), cap_add]);
        }

        for cap_drop in self.cap_drop.iter().cloned() {
            args.extend([String::from("--cap-drop"), cap_drop]);
        }

        if let Some(cgroup) = self.cgroup.as_ref().cloned() {
            args.extend([String::from("--cgroupns"), cgroup]);
        }

        if let Some(cgroup_parent) = self.cgroup_parent.as_ref().cloned() {
            args.extend([String::from("--cgroup-parent"), cgroup_parent]);
        }

        if let Some(cpu_period) = self.cpu_period {
            args.extend([
                String::from("--cpu-period"),
                cpu_period.as_micros().to_string(),
            ]);
        }

        if let Some(cpu_quota) = self.cpu_quota {
            args.extend([
                String::from("--cpu-quota"),
                cpu_quota.as_micros().to_string(),
            ]);
        }

        if let Some(cpu_rt_period) = self.cpu_rt_period {
            args.extend([
                String::from("--cpu-rt-period"),
                cpu_rt_period.as_micros().to_string(),
            ]);
        }

        if let Some(cpu_rt_runtime) = self.cpu_rt_runtime {
            args.extend([
                String::from("--cpu-rt-runtime"),
                cpu_rt_runtime.as_micros().to_string(),
            ]);
        }

        if let Some(cpu_shares) = self.cpu_shares {
            args.extend([String::from("--cpu-shares"), cpu_shares.to_string()]);
        }

        if let Some(cpuset) = self.cpuset.as_ref().cloned() {
            args.extend([String::from("--cpuset-cpus"), cpuset]);
        }

        let mut is_deploy_v3 = [false; 4];

        if let Some(deploy) = &self.deploy {
            if let Some(resources) = &deploy.resources {
                if let Some(limits) = &resources.limits {
                    if let Some(memory) = limits.memory {
                        args.extend([String::from("--memory"), memory.to_string()]);
                        is_deploy_v3[0] = true;
                    }
                }

                if let Some(reservations) = &resources.reservations {
                    if let Some(cpus) = reservations.cpus {
                        args.extend([String::from("--cpus"), cpus.to_string()]);
                        is_deploy_v3[1] = true;
                    }

                    if let Some(memory) = reservations.memory {
                        args.extend([String::from("--memory-reservation"), memory.to_string()]);
                        is_deploy_v3[2] = true;
                    }

                    if let Some(pids) = reservations.pids {
                        args.extend([String::from("--pids-limit"), pids.to_string()]);
                        is_deploy_v3[3] = true;
                    }
                }
            }
        }

        if !is_deploy_v3[0] {
            if let Some(mem_limit) = self.mem_limit {
                args.extend([String::from("--memory"), mem_limit.to_string()]);
            }
        }

        if !is_deploy_v3[1] {
            if let Some(cpus) = self.cpus {
                args.extend([String::from("--cpus"), cpus.to_string()]);
            }
        }

        if !is_deploy_v3[2] {
            if let Some(mem_reservation) = self.mem_reservation {
                args.extend([
                    String::from("--memory-reservation"),
                    mem_reservation.to_string(),
                ]);
            }
        }

        if !is_deploy_v3[3] {
            if let Some(pids_limit) = self.pids_limit {
                args.extend([String::from("--pids-limit"), pids_limit.to_string()]);
            }
        }

        for device_cgroup_rule in self.device_cgroup_rules.iter().cloned() {
            args.extend([String::from("--device-cgroup-rule"), device_cgroup_rule]);
        }

        for device in &self.devices {
            args.extend([String::from("--device"), device.to_string()]);
        }

        for dns in self.dns.iter().cloned() {
            args.extend([String::from("--dns"), dns]);
        }

        for dns_opt in self.dns_opt.iter().cloned() {
            args.extend([String::from("--dns-option"), dns_opt]);
        }

        for dns_search in self.dns_search.iter().cloned() {
            args.extend([String::from("--dns-search"), dns_search]);
        }

        if !self.entrypoint.is_empty() {
            args.extend([
                String::from("--entrypoint"),
                serde_json::to_string(&self.entrypoint).unwrap(),
            ]);
        }

        for env_file in &self.env_file {
            args.extend([
                String::from("--env-file"),
                env_file.to_string_lossy().to_string(),
            ]);
        }

        for (key, value) in &self.environment {
            args.extend([
                String::from("--env"),
                if let Some(value) = value {
                    format!("{key}={value}")
                } else {
                    key.clone()
                },
            ]);
        }

        for expose in self.expose.iter().cloned() {
            args.extend([String::from("--expose"), expose]);
        }

        for (host, ip) in &self.extra_hosts {
            args.extend([String::from("--add-host"), format!("{host}:{ip}")]);
        }

        for group_add in self.group_add.iter().cloned() {
            args.extend([String::from("--group-add"), group_add]);
        }

        if let Some(healthcheck) = &self.healthcheck {
            if !healthcheck.test.is_empty() {
                args.extend([
                    String::from("--health-cmd"),
                    serde_json::to_string(&healthcheck.test).unwrap(),
                ]);
            }

            if let Some(interval) = healthcheck.interval {
                args.extend([
                    String::from("--health-interval"),
                    interval.as_secs().to_string(),
                ]);
            }

            if let Some(timeout) = healthcheck.timeout {
                args.extend([
                    String::from("--health-timeout"),
                    timeout.as_secs().to_string(),
                ]);
            }

            if let Some(start_period) = healthcheck.start_period {
                args.extend([
                    String::from("--health-start-period"),
                    start_period.as_secs().to_string(),
                ]);
            }

            if let Some(retries) = healthcheck.retries {
                args.extend([String::from("--health-retries"), retries.to_string()]);
            }

            if healthcheck.disable.unwrap_or_default() {
                args.push(String::from("--no-healthcheck"));
            }
        }

        if let Some(hostname) = self.hostname.as_ref().cloned() {
            args.extend([String::from("--hostname"), hostname]);
        }

        if self.init.unwrap_or_default() {
            args.push(String::from("--init"));
        }

        if let Some(ipc) = self.ipc.as_ref().cloned() {
            args.extend([String::from("--ipc"), ipc]);
        }

        for (key, value) in &self.labels {
            args.extend([String::from("--label"), format!("{key}={value}")]);
        }

        if let Some(logging) = &self.logging {
            if let Some(driver) = logging.driver.as_ref().cloned() {
                args.extend([String::from("--log-driver"), driver]);
            }

            for (key, value) in &logging.options {
                args.extend([String::from("--log-opt"), format!("{key}={value}")]);
            }
        }

        if let Some(mem_swappiness) = self.mem_swappiness {
            args.extend([
                String::from("--memory-swappiness"),
                mem_swappiness.to_string(),
            ]);
        }

        if let Some(memswap_limit) = &self.memswap_limit {
            args.extend([String::from("--memory-swap"), memswap_limit.to_string()]);
        }

        if let Some(network_mode) = self.network_mode.as_ref().cloned() {
            args.extend([String::from("--network"), network_mode]);
        }

        if self.oom_kill_disable.unwrap_or_default() {
            args.push(String::from("--oom-kill-disable"));
        }

        if let Some(oom_score_adj) = self.oom_score_adj {
            args.extend([String::from("--oom-score-adj"), oom_score_adj.to_string()]);
        }

        if let Some(pid) = self.pid.as_ref().cloned() {
            args.extend([String::from("--pid"), pid]);
        }

        if let Some(platform) = self.platform.as_ref().cloned() {
            args.extend([String::from("--platform"), platform]);
        }

        for port in &self.ports {
            args.extend([String::from("--publish"), port.to_string()]);
        }

        if self.privileged.unwrap_or_default() {
            args.push(String::from("--privileged"));
        }

        if self.read_only.unwrap_or_default() {
            args.push(String::from("--read-only"));
        }

        if let Some(restart) = &self.restart {
            args.extend([String::from("--restart"), restart.to_string()]);
        }

        if let Some(runtime) = self.runtime.as_ref().cloned() {
            global_args.extend([String::from("--runtime"), runtime]);
        }

        for (key, value) in &self.security_opt {
            args.extend([
                String::from("--security-opt"),
                if let Some(value) = value {
                    format!("{key}={value}")
                } else {
                    key.clone()
                },
            ]);
        }

        if let Some(shm_size) = self.shm_size {
            args.extend([String::from("--shm-size"), shm_size.to_string()]);
        }

        if self.stdin_open.unwrap_or_default() {
            args.push(String::from("--interactive"));
        }

        if let Some(stop_grace_period) = self.stop_grace_period {
            args.extend([
                String::from("--stop-timeout"),
                stop_grace_period.as_secs().to_string(),
            ]);
        }

        if let Some(stop_signal) = self.stop_signal.as_ref().cloned() {
            args.extend([String::from("--stop-signal"), stop_signal]);
        }

        for (key, value) in &self.storage_opt {
            global_args.extend([String::from("--storage-opt"), format!("{key}={value}")]);
        }

        for (key, value) in &self.sysctls {
            args.extend([String::from("--sysctl"), format!("{key}={value}")]);
        }

        for tmpfs in &self.tmpfs {
            args.extend([String::from("--tmpfs"), tmpfs.to_string_lossy().to_string()]);
        }

        if self.tty.unwrap_or_default() {
            args.push(String::from("--tty"));
        }

        for (key, value) in &self.ulimits {
            args.extend([String::from("--ulimit"), format!("{key}={value}")]);
        }

        if let Some(user) = self.user.as_ref().cloned() {
            args.extend([String::from("--user"), user]);
        }

        if let Some(userns_mode) = self.userns_mode.as_ref().cloned() {
            args.extend([String::from("--userns"), userns_mode]);
        }

        if let Some(uts) = self.uts.as_ref().cloned() {
            args.extend([String::from("--uts"), uts]);
        }

        for volume in self.volumes_from.iter().cloned() {
            args.extend([String::from("--volumes-from"), volume]);
        }

        if let Some(working_dir) = &self.working_dir {
            args.extend([
                String::from("--workdir"),
                working_dir.to_string_lossy().to_string(),
            ]);
        }

        if let Some(image) = self.image.as_ref().cloned() {
            args.push(image);
        }

        if !self.command.is_empty() {
            args.extend(self.command.clone());
        }

        (global_args, args)
    }
}

#[skip_serializing_none]
#[serde_with::apply(
    Vec => #[serde(skip_serializing_if = "Vec::is_empty", default)]
)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct BlkioConfig {
    pub(crate) weight: Option<u16>,
    pub(crate) weight_device: Vec<WeightDevice>,
    pub(crate) device_read_bps: Vec<ThrottleDevice>,
    pub(crate) device_write_bps: Vec<ThrottleDevice>,
    pub(crate) device_read_iops: Vec<ThrottleDevice>,
    pub(crate) device_write_iops: Vec<ThrottleDevice>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct WeightDevice {
    #[serde_as(as = "AbsPathBuf")]
    pub(crate) path: PathBuf,
    pub(crate) weight: u16,
}

impl Display for WeightDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path.display(), self.weight)
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ThrottleDevice {
    #[serde_as(as = "AbsPathBuf")]
    pub(crate) path: PathBuf,
    pub(crate) rate: Byte,
}

impl Display for ThrottleDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path.display(), self.rate)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Dependency {
    pub(crate) condition: Condition,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub(crate) enum Condition {
    #[serde(rename = "service_started")]
    Started,
    #[serde(rename = "service_healthy")]
    Healthy,
    #[serde(rename = "service_completed_successfully")]
    CompletedSuccessfully,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct DeployConfig {
    pub(crate) replicas: Option<u32>,
    pub(crate) resources: Option<Resources>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Resources {
    pub(crate) limits: Option<Resource>,
    pub(crate) reservations: Option<Resource>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Resource {
    #[serde_as(as = "Option<PickFirst<(_, DisplayFromStr)>>")]
    pub(crate) cpus: Option<f64>,
    pub(crate) memory: Option<Byte>,
    pub(crate) pids: Option<i32>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Device {
    #[serde_as(as = "AbsPathBuf")]
    pub(crate) source: PathBuf,
    pub(crate) target: Option<PathBuf>,
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

impl Display for Device {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut device = vec![self.source.to_string_lossy().to_string()];

        if let Some(target) = &self.target {
            device.push(target.to_string_lossy().to_string());
        }

        if let Some(permissions) = &self.permissions {
            device.push(permissions.clone());
        }

        write!(f, "{}", device.join(":"))
    }
}

#[skip_serializing_none]
#[serde_as]
#[serde_with::apply(
    Vec => #[serde(skip_serializing_if = "Vec::is_empty", default)]
)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Healthcheck {
    #[serde_as(as = "PickFirst<(_, CommandOrString)>")]
    pub(crate) test: Vec<String>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) interval: Option<Duration>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) timeout: Option<Duration>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) start_period: Option<Duration>,
    pub(crate) retries: Option<u32>,
    pub(crate) disable: Option<bool>,
}

#[skip_serializing_none]
#[serde_with::apply(
    IndexMap => #[serde(skip_serializing_if = "IndexMap::is_empty", default)]
)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Logging {
    pub(crate) driver: Option<String>,
    pub(crate) options: IndexMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub(crate) enum SwapLimit {
    Limited(Byte),
    Unlimited(i8),
}

impl Display for SwapLimit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SwapLimit::Limited(limit) => write!(f, "{limit}"),
            SwapLimit::Unlimited(_) => write!(f, "{}", -1),
        }
    }
}

#[skip_serializing_none]
#[serde_as]
#[serde_with::apply(
    Vec => #[serde(skip_serializing_if = "Vec::is_empty", default)]
)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ServiceNetwork {
    #[serde_as(as = "Vec<DisplayFromAny>")]
    pub(crate) aliases: Vec<String>,
    #[serde_as(as = "Option<DisplayFromAny>")]
    pub(crate) ipv4_address: Option<String>,
    #[serde_as(as = "Option<DisplayFromAny>")]
    pub(crate) ipv6_address: Option<String>,
}

impl Display for ServiceNetwork {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut options = Vec::new();

        for alias in &self.aliases {
            options.push(format!("alias={alias}"));
        }

        if let Some(ipv4_address) = &self.ipv4_address {
            options.push(format!("ip={ipv4_address}"));
        }

        if let Some(ipv6_address) = &self.ipv6_address {
            options.push(format!("ip={ipv6_address}"));
        }

        if options.is_empty() {
            write!(f, "")
        } else {
            write!(f, ":{}", options.join(","))
        }
    }
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub(crate) struct Port {
    #[serde_as(as = "DisplayFromAny")]
    pub(crate) target: String,
    #[serde_as(as = "Option<DisplayFromAny>")]
    pub(crate) published: Option<String>,
    #[serde_as(as = "Option<DisplayFromAny>")]
    pub(crate) host_ip: Option<String>,
    #[serde_as(as = "DisplayFromAny")]
    #[serde(default = "default_protocol")]
    pub(crate) protocol: String,
}

fn default_protocol() -> String {
    String::from("tcp")
}

impl Display for Port {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut port = self.target.clone();

        match (&self.published, &self.host_ip) {
            (None, None) => {}
            (published, host_ip) => {
                port = format!("{}:{port}", published.clone().unwrap_or_default());

                if let Some(host_ip) = host_ip {
                    port = format!("{host_ip}:{port}");
                }
            }
        }

        if self.protocol != "tcp" {
            port = format!("{port}/{}", self.protocol);
        }

        write!(f, "{port}")
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PullPolicy {
    Always,
    Never,
    Missing,
    Newer,
}

impl Display for PullPolicy {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", AsKebabCase(format!("{self:?}")))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RestartPolicy {
    No,
    Always,
    OnFailure,
    UnlessStopped,
}

impl Display for RestartPolicy {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", AsKebabCase(format!("{self:?}")))
    }
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub(crate) struct FileReference {
    #[serde_as(as = "DisplayFromAny")]
    pub(crate) source: String,
    #[serde_as(as = "Option<DisplayFromAny>")]
    pub(crate) target: Option<String>,
    #[serde_as(as = "Option<DisplayFromAny>")]
    pub(crate) uid: Option<String>,
    #[serde_as(as = "Option<DisplayFromAny>")]
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

impl Display for FileReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut file_reference = vec![self.source.clone()];

        if let Some(target) = &self.target {
            file_reference.push(format!("target={target}"));
        }

        if let Some(uid) = &self.uid {
            file_reference.push(format!("uid={uid}"));
        }

        if let Some(gid) = &self.gid {
            file_reference.push(format!("gid={gid}"));
        }

        if let Some(mode) = &self.mode {
            file_reference.push(format!("mode={mode}"));
        }

        write!(f, "{}", file_reference.join(","))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub(crate) enum ResourceLimit {
    Single(i32),
    Double { soft: i32, hard: i32 },
}

impl Display for ResourceLimit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ResourceLimit::Single(limit) => write!(f, "{limit}"),
            ResourceLimit::Double { soft, hard } => write!(f, "{soft}:{hard}"),
        }
    }
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ServiceVolume {
    #[serde(flatten)]
    pub(crate) r#type: ServiceVolumeType,
    #[serde_as(as = "DisplayFromAny")]
    pub(crate) target: PathBuf,
    pub(crate) read_only: Option<bool>,
    pub(crate) volume: Option<ServiceVolumeVolume>,
    pub(crate) bind: Option<ServiceVolumeBind>,
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

impl Display for ServiceVolume {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut volume = vec![self.target.to_string_lossy().to_string()];
        let mut options = Vec::new();

        match &self.r#type {
            ServiceVolumeType::Volume(source) => {
                if let Some(source) = source {
                    volume.insert(0, source.clone());

                    if let Some(volume) = &self.volume {
                        if volume.nocopy.unwrap_or_default() {
                            options.push(String::from("nocopy"));
                        }
                    }
                }
            }
            ServiceVolumeType::Bind(source) => {
                volume.insert(0, source.to_string_lossy().to_string());

                if let Some(bind) = &self.bind {
                    if let Some(propagation) = bind.propagation.as_ref().cloned() {
                        options.push(propagation);
                    }

                    if let Some(selinux) = bind.selinux.as_ref().cloned() {
                        options.push(selinux);
                    }
                }
            }
            ServiceVolumeType::Tmpfs => {
                if let Some(tmpfs) = &self.tmpfs {
                    if let Some(size) = tmpfs.size {
                        options.push(format!("size={size}"));
                    }

                    if let Some(mode) = tmpfs.mode {
                        options.push(format!("mode={mode}"));
                    }
                }
            }
        }

        if self.read_only.unwrap_or_default() {
            options.push(String::from("ro"));
        }

        if options.is_empty() {
            write!(f, "{}", volume.join(":"))
        } else {
            write!(f, "{}:{}", volume.join(":"), options.join(","))
        }
    }
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", content = "source", rename_all = "lowercase")]
pub(crate) enum ServiceVolumeType {
    Volume(#[serde_as(as = "Option<DisplayFromAny>")] Option<String>),
    Bind(#[serde_as(as = "PickFirst<(AbsPathBuf, DisplayFromAny)>")] PathBuf),
    Tmpfs,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ServiceVolumeVolume {
    pub(crate) nocopy: Option<bool>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub(crate) struct ServiceVolumeBind {
    #[serde_as(as = "Option<DisplayFromAny>")]
    pub(crate) propagation: Option<String>,
    pub(crate) create_host_path: Option<bool>,
    #[serde_as(as = "Option<DisplayFromAny>")]
    pub(crate) selinux: Option<String>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ServiceVolumeTmpfs {
    pub(crate) size: Option<Byte>,
    #[serde_as(as = "Option<PickFirst<(_, DisplayFromStr)>>")]
    pub(crate) mode: Option<u32>,
}

#[skip_serializing_none]
#[serde_as]
#[serde_with::apply(
    IndexMap => #[serde(skip_serializing_if = "IndexMap::is_empty", default)]
)]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct Network {
    pub(crate) name: Option<String>,
    pub(crate) driver: Option<String>,
    pub(crate) driver_opts: IndexMap<String, String>,
    pub(crate) enable_ipv6: Option<bool>,
    pub(crate) ipam: Option<IpamConfig>,
    pub(crate) internal: Option<bool>,
    #[serde_as(
        as = "PickFirst<(_, IndexMap<DisplayFromAny, DisplayFromAny>, MappingWithEqualsEmpty)>"
    )]
    pub(crate) labels: IndexMap<String, String>,
    pub(crate) external: Option<bool>,
}

impl Network {
    pub(crate) fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if !self.external.unwrap_or_default() {
            if let Some(driver) = self.driver.as_ref().cloned() {
                args.extend([String::from("--driver"), driver]);
            }

            for (key, value) in &self.driver_opts {
                args.extend([String::from("--opt"), format!("{key}={value}")]);
            }

            if self.enable_ipv6.unwrap_or_default() {
                args.push(String::from("--ipv6"));
            }

            if let Some(ipam) = &self.ipam {
                if let Some(driver) = ipam.driver.as_ref().cloned() {
                    args.extend([String::from("--ipam-driver"), driver]);
                }

                for config in &ipam.config {
                    if let Some(subnet) = config.subnet.as_ref().cloned() {
                        args.extend([String::from("--subnet"), subnet]);
                    }

                    if let Some(ip_range) = config.ip_range.as_ref().cloned() {
                        args.extend([String::from("--ip-range"), ip_range]);
                    }

                    if let Some(gateway) = config.gateway.as_ref().cloned() {
                        args.extend([String::from("--gateway"), gateway]);
                    }
                }
            }

            if self.internal.unwrap_or_default() {
                args.push(String::from("--internal"));
            }

            for (key, value) in &self.labels {
                args.extend([String::from("--label"), format!("{key}={value}")]);
            }

            args.push(self.name.clone().unwrap());
        }

        args
    }
}

#[skip_serializing_none]
#[serde_with::apply(
    Vec => #[serde(skip_serializing_if = "Vec::is_empty", default)]
)]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct IpamConfig {
    pub(crate) driver: Option<String>,
    pub(crate) config: Vec<IpamPool>,
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
#[serde_with::apply(
    IndexMap => #[serde(skip_serializing_if = "IndexMap::is_empty", default)]
)]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct Volume {
    pub(crate) name: Option<String>,
    pub(crate) driver: Option<String>,
    pub(crate) driver_opts: IndexMap<String, String>,
    pub(crate) external: Option<bool>,
    #[serde_as(
        as = "PickFirst<(_, IndexMap<DisplayFromAny, DisplayFromAny>, MappingWithEqualsEmpty)>"
    )]
    pub(crate) labels: IndexMap<String, String>,
}

impl Volume {
    pub(crate) fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if !self.external.unwrap_or_default() {
            if let Some(driver) = self.driver.as_ref().cloned() {
                args.extend([String::from("--driver"), driver]);
            }

            for (key, value) in &self.driver_opts {
                args.extend([String::from("--opt"), format!("{key}={value}")]);
            }

            for (key, value) in &self.labels {
                args.extend([String::from("--label"), format!("{key}={value}")]);
            }

            args.push(self.name.clone().unwrap());
        }

        args
    }
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Secret {
    pub(crate) name: Option<String>,
    #[serde_as(as = "Option<AbsPathBuf>")]
    pub(crate) file: Option<PathBuf>,
    pub(crate) environment: Option<String>,
    pub(crate) external: Option<bool>,
}

impl Secret {
    pub(crate) fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if !self.external.unwrap_or_default() {
            if self.environment.is_some() {
                args.push(String::from("--env"));
            }

            args.push(self.name.clone().unwrap());

            if let Some(environment) = self.environment.as_ref().cloned() {
                args.push(environment);
            } else if let Some(file) = &self.file {
                args.push(file.to_string_lossy().to_string());
            }
        }

        args
    }
}

serde_conv!(
    AbsPathBuf,
    PathBuf,
    |path: &PathBuf| path.to_string_lossy().to_string(),
    |path: String| -> Result<_> {
        Path::new(&path)
            .absolutize()
            .map_err(Error::from)
            .map(PathBuf::from)
    }
);

serde_conv!(
    CommandOrString,
    Vec<String>,
    shell_words::join,
    |args: String| { shell_words::split(&args).map_err(Error::from) }
);

serde_conv!(
    DependsOnVec,
    IndexMap<String, Dependency>,
    |dependencies: &IndexMap<String, Dependency>| {
        dependencies.keys().cloned().collect::<Vec<_>>()
    },
    |dependencies: Vec<String>| -> Result<_, Infallible> {
        Ok(dependencies
            .into_iter()
            .map(|dependency| {
                (
                    dependency,
                    Dependency {
                        condition: Condition::Started,
                    },
                )
            })
            .collect::<IndexMap<_, _>>())
    }
);

serde_conv!(
    DeviceOrString,
    Device,
    ToString::to_string,
    |device: String| -> Result<_> {
        let mut parts = device.split(':');

        Ok(Device {
            source: Path::new(parts.next().unwrap()).absolutize()?.to_path_buf(),
            target: parts.next().map(PathBuf::from),
            permissions: parts.next().map(ToString::to_string),
        })
    }
);

serde_conv!(
    DurationWithSuffix,
    Duration,
    |duration: &Duration| format_duration(*duration).to_string(),
    |duration: String| parse_duration(&duration)
);

serde_conv!(
    FileReferenceOrString,
    FileReference,
    |file_reference: &FileReference| file_reference.source.clone(),
    |source| -> Result<_, Infallible> {
        Ok(FileReference {
            source,
            ..FileReference::default()
        })
    }
);

serde_conv!(
    MappingWithColonEmpty,
    IndexMap<String, String>,
    |variables: &IndexMap<String, String>| {
        variables
            .iter()
            .map(|(key, value)| {
                if value.is_empty() {
                    key.clone()
                } else {
                    format!("{key}: {value}")
                }
            })
            .collect::<Vec<_>>()
    },
    |variables: Vec<String>| -> Result<_, Infallible> {
        Ok(variables
            .into_iter()
            .map(|variable| {
                let mut parts = variable.split(':');
                (
                    parts.next().unwrap().to_string(),
                    parts.next().map(ToString::to_string).unwrap_or_default(),
                )
            })
            .collect::<IndexMap<_, _>>())
    }
);

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
    |variables: Vec<String>| -> Result<_, Infallible> {
        Ok(variables
            .into_iter()
            .map(|variable| {
                let mut parts = variable.split('=');
                (
                    parts.next().unwrap().to_string(),
                    parts.next().map(ToString::to_string).unwrap_or_default(),
                )
            })
            .collect::<IndexMap<_, _>>())
    }
);

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
        let variables = variables
            .into_iter()
            .map(|variable| -> Result<_> {
                let mut parts = variable.split('=');
                let key = parts.next().unwrap().to_string();
                let value = parts
                    .next()
                    .map(ToString::to_string)
                    .ok_or_else(|| anyhow!("{key}: value not defined"))?;

                Ok((key, value))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(variables.into_iter().collect::<IndexMap<_, _>>())
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
    |variables: Vec<String>| -> Result<_, Infallible> {
        Ok(variables
            .into_iter()
            .map(|variable| {
                let mut parts = variable.split('=');
                (
                    parts.next().unwrap().to_string(),
                    parts.next().map(ToString::to_string),
                )
            })
            .collect::<IndexMap<_, _>>())
    }
);

serde_conv!(
    NetworksVec,
    IndexMap<String, Option<ServiceNetwork>>,
    |networks: &IndexMap<String, Option<ServiceNetwork>>| {
        networks.keys().cloned().collect::<Vec<_>>()
    },
    |networks: Vec<String>| -> Result<_, Infallible> {
        Ok(networks
            .into_iter()
            .map(|network| (network, None))
            .collect::<IndexMap<_, _>>())
    }
);

pub(crate) fn parse_port(port: &str) -> Result<Port, Infallible> {
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
        protocol: container_parts
            .next()
            .map_or_else(|| String::from("tcp"), ToString::to_string),
    })
}

serde_conv!(PortOrString, Port, ToString::to_string, |port: String| {
    parse_port(&port)
});

serde_conv!(
    PortOrU16,
    Port,
    |port: &Port| port.target.parse::<u16>().unwrap(),
    |target: u16| -> Result<_, Infallible> {
        Ok(Port {
            target: target.to_string(),
            protocol: String::from("tcp"),
            ..Port::default()
        })
    }
);

serde_conv!(
    SecurityOptVec,
    Vec<(String, Option<String>)>,
    |security_opts: &Vec<(String, Option<String>)>| {
        security_opts
            .iter()
            .map(|(key, value)| {
                if let Some(value) = value {
                    format!("{key}:{value}")
                } else {
                    key.clone()
                }
            })
            .collect::<Vec<_>>()
    },
    |security_opts: Vec<String>| -> Result<_, Infallible> {
        Ok(security_opts
            .into_iter()
            .map(|security_opt| {
                if let Some(idx) = security_opt.find(':') {
                    (
                        security_opt[..idx].to_string(),
                        Some(security_opt[idx + 1..].to_string()),
                    )
                } else {
                    (security_opt, None)
                }
            })
            .collect::<Vec<_>>())
    }
);

pub(crate) fn parse_service_volume(mount: &str) -> Result<ServiceVolume> {
    let mut r#type = ServiceVolumeType::Volume(None);
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
                r#type = ServiceVolumeType::Bind(Path::new(src).absolutize()?.to_path_buf());
                bind = Some(ServiceVolumeBind {
                    create_host_path: Some(true),
                    ..ServiceVolumeBind::default()
                });
            } else {
                r#type = ServiceVolumeType::Volume(Some(src.to_string()));
            }

            target = dst.to_string();
        }
        [dst, opts] => {
            target = dst.to_string();
            options = opts;
        }
        [src, dst, opts] => {
            if src.starts_with('/') || src.starts_with('.') {
                r#type = ServiceVolumeType::Bind(Path::new(src).absolutize()?.to_path_buf());
                bind = Some(ServiceVolumeBind {
                    create_host_path: Some(true),
                    ..ServiceVolumeBind::default()
                });
            } else {
                r#type = ServiceVolumeType::Volume(Some(src.to_string()));
            }

            target = dst.to_string();
            options = opts;
        }
        _ => {
            bail!("{mount}: too many colons");
        }
    }

    let options = options.split(',');
    let mut unused = Vec::new();

    for option in options {
        match option {
            "rw" | "ro" => {
                read_only = Some(option == "ro");
            }
            "shared" | "rshared" | "slave" | "rslave" | "private" | "rprivate" | "unbindable"
            | "runbindable" => {
                bind.get_or_insert_with(ServiceVolumeBind::default)
                    .propagation = Some(option.to_string());
            }
            "z" | "Z" => {
                bind.get_or_insert_with(ServiceVolumeBind::default).selinux =
                    Some(option.to_string());
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
            *STYLED_WARNING,
            unused.join(", ")
        );
    }

    Ok(ServiceVolume {
        r#type,
        target: PathBuf::from(target),
        read_only,
        bind,
        volume,
        tmpfs: None,
    })
}

serde_conv!(
    ServiceVolumeOrString,
    ServiceVolume,
    ToString::to_string,
    |mount: String| parse_service_volume(&mount)
);

#[cfg(test)]
mod tests {
    use std::fs;

    use assert_matches::assert_matches;
    use pretty_assertions::assert_eq;
    use test_generator::test_resources;

    use super::*;

    #[test_resources("tests/fixtures/**/*.y*ml")]
    fn serde(resource: &str) {
        let contents = fs::read_to_string(resource).unwrap();

        assert_matches!(serde_yaml::from_str::<Compose>(&contents), Ok(_));
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
