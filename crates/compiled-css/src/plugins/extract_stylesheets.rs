//! Port of `packages/css/src/plugins/extract-stylesheets.ts`.
//!
//! Iteration order matters — sheets feed into downstream hashing.

use postcss_core::Root;

#[derive(Debug, Clone, Default)]
pub struct ExtractStyleSheetsOpts {
    /// Sheets produced by the run are pushed here in order.
    pub sheets: Vec<String>,
}

pub fn extract_stylesheets(_root: &mut Root, _opts: &mut ExtractStyleSheetsOpts) {
    unimplemented!("Phase 4a — port extract-stylesheets.ts");
}
