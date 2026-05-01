//! Port of `packages/css/src/plugins/sort-atomic-style-sheet.ts`.

use postcss_core::Root;

#[derive(Debug, Clone, Default)]
pub struct SortAtomicStyleSheetOpts {
    /// `undefined` upstream means "use plugin default" — we mirror with
    /// `Option<bool>`. Default values live in the plugin port itself, not at
    /// the call site (matches upstream comment in `sort.ts:18-26`).
    pub sort_at_rules_enabled: Option<bool>,
    pub sort_shorthand_enabled: Option<bool>,
}

pub fn sort_atomic_style_sheet(_root: &mut Root, _opts: &SortAtomicStyleSheetOpts) {
    unimplemented!("Phase 4c — port sort-atomic-style-sheet.ts");
}
