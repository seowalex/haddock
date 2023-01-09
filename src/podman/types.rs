use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Version {
    pub(crate) client: VersionClient,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct VersionClient {
    pub(crate) version: semver::Version,
}
