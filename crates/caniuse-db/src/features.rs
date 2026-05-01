//! Caniuse-lite feature table.
//!
//! Data is loaded from the embedded JSON snapshot (`OUT_DIR/features_data.json`,
//! produced by `build.rs`) on first access via `once_cell::Lazy`.

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Feature {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub shown: bool,
    #[serde(default)]
    pub stats: IndexMap<String, IndexMap<String, String>>,
}

#[derive(Debug, Default, Deserialize)]
struct Snapshot {
    #[serde(rename = "caniuseLiteVersion", default)]
    pub caniuse_lite_version: String,
    #[serde(default)]
    pub features: IndexMap<String, Feature>,
    #[serde(default)]
    pub agents: IndexMap<String, super::agents::Agent>,
    #[serde(default)]
    pub browsers: IndexMap<String, String>,
}

const RAW: &str = include_str!(concat!(env!("OUT_DIR"), "/features_data.json"));

static SNAPSHOT: Lazy<Snapshot> = Lazy::new(|| {
    serde_json::from_str(RAW).expect("caniuse-lite snapshot must be valid JSON")
});

pub static FEATURES: Lazy<&'static IndexMap<String, Feature>> =
    Lazy::new(|| &SNAPSHOT.features);

pub fn feature(name: &str) -> Option<&'static Feature> { FEATURES.get(name) }

pub fn list() -> Vec<&'static String> { FEATURES.keys().collect() }

pub(crate) fn snapshot_agents() -> &'static IndexMap<String, super::agents::Agent> {
    &SNAPSHOT.agents
}

pub(crate) fn snapshot_browsers() -> &'static IndexMap<String, String> {
    &SNAPSHOT.browsers
}

pub fn snapshot_version() -> &'static str { SNAPSHOT.caniuse_lite_version.as_str() }
