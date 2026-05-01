//! Port of `packages/css/src/generate-compression-map.ts`.
//!
//! Re-exported by `packages/css/src/index.ts:9`. Body deferred — this is on
//! the public surface but not on `transformCss` / `sort`'s direct hashing
//! path; ports lazily as consumers reach it.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenerateCompressionMapOpts;

pub fn generate_compression_map(
    _files: &[(String, String)],
    _opts: &GenerateCompressionMapOpts,
) -> IndexMap<String, String> {
    unimplemented!("Port `packages/css/src/generate-compression-map.ts` when consumer demands it");
}
