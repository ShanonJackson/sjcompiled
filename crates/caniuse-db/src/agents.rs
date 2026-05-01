//! Caniuse-lite agents table — populated from the same JSON snapshot as
//! features. Each agent carries its version list, prefix, release dates,
//! and global usage map.

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Agent {
    #[serde(default)]
    pub browser: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub versions: Vec<Option<String>>,
    #[serde(default, rename = "usage_global")]
    pub usage_global: IndexMap<String, f64>,
    #[serde(default)]
    pub release_date: IndexMap<String, Option<i64>>,
    #[serde(default)]
    pub prefix_exceptions: IndexMap<String, String>,
}

pub static AGENTS: Lazy<&'static IndexMap<String, Agent>> =
    Lazy::new(super::features::snapshot_agents);

pub fn agent(name: &str) -> Option<&'static Agent> { AGENTS.get(name) }

pub static BROWSERS: Lazy<&'static IndexMap<String, String>> =
    Lazy::new(super::features::snapshot_browsers);
