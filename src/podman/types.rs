use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct PsContainer {
    pub(crate) labels: IndexMap<String, String>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Version {
    pub(crate) client: VersionClient,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct VersionClient {
    pub(crate) version: semver::Version,
}
