use std::collections::VecDeque;

use serde::Deserialize;
use serde_with::{serde_as, with_prefix, DisplayFromStr};

with_prefix!(prefix_io_podman_compose "io.podman.compose.");

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Container {
    pub(crate) id: String,
    #[serde(rename = "ImageID")]
    pub(crate) image_id: String,
    #[serde(with = "prefix_io_podman_compose")]
    pub(crate) labels: Option<ContainerLabels>,
    pub(crate) names: VecDeque<String>,
}

#[serde_as]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ContainerLabels {
    pub(crate) service: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub(crate) container_number: Option<usize>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct Network {
    pub(crate) name: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Pod {
    #[serde(with = "prefix_io_podman_compose")]
    pub(crate) labels: Option<PodLabels>,
}

#[serde_as]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PodLabels {
    pub(crate) config_hash: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Version {
    pub(crate) client: VersionClient,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct VersionClient {
    pub(crate) version: semver::Version,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Volume {
    pub(crate) name: String,
}
