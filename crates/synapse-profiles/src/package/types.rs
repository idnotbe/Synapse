use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use synapse_core::{Backend, ProfileId, ProfileUseScope};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfilePackageManifest {
    pub schema_version: u32,
    pub kind: String,
    pub package_id: String,
    pub package_version: String,
    pub profile_id: ProfileId,
    pub profile_version: String,
    pub created_at: String,
    pub author: PackageAuthor,
    pub source: PackageSource,
    #[serde(default)]
    pub targets: Vec<PackageTarget>,
    pub assumptions: PackageAssumptions,
    pub input: PackageInput,
    pub permissions: PackagePermissions,
    #[serde(default)]
    pub changelog: Vec<PackageChangelogEntry>,
    pub hashes: PackageHashes,
    pub files: PackageFiles,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageAuthor {
    pub name: String,
    pub contact: String,
    pub attribution: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSource {
    pub kind: String,
    pub uri: String,
    pub revision: String,
    pub built_by: String,
    pub generated_by: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageTarget {
    pub target_id: String,
    pub target_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_regex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_appid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageAssumptions {
    pub os: String,
    pub synapse_schema_version: u32,
    pub display: DisplayAssumptions,
    #[serde(default)]
    pub benchmark_ids: Vec<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayAssumptions {
    pub min_width: u32,
    pub min_height: u32,
    pub dpi_scale_min: f32,
    pub dpi_scale_max: f32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageInput {
    pub backends: Vec<Backend>,
    #[serde(default)]
    pub firmware: Vec<PackageDependency>,
    #[serde(default)]
    pub models: Vec<PackageDependency>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageDependency {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagePermissions {
    pub license_spdx: String,
    pub contribution_terms: String,
    pub use_scope: ProfileUseScope,
    pub execution: PackageExecutionPermissions,
    pub contribution: PackageContributionPermissions,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageExecutionPermissions {
    pub local_only: bool,
    pub remote_server_allowed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageContributionPermissions {
    pub export_allowed: bool,
    pub share_audit_allowed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageChangelogEntry {
    pub version: String,
    pub at: String,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageHashes {
    pub profile_toml_sha256: String,
    pub package_sha256: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageFiles {
    pub profile_toml: String,
    #[serde(default)]
    pub assets: Vec<String>,
}
