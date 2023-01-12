use std::collections::VecDeque;

use serde::Deserialize;
use serde_with::with_prefix;

with_prefix!(prefix_io_podman_compose "io.podman.compose.");

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Container {
    #[serde(with = "prefix_io_podman_compose")]
    pub(crate) labels: ContainerLabels,
    pub(crate) names: VecDeque<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct ContainerLabels {
    pub(crate) service: Option<String>,
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
