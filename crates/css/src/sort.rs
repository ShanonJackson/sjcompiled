//! Port of `packages/css/src/sort.ts`.
//!
//! Second hashing entry point per `PARITY_VERSIONS.md`. Pipeline:
//!
//! ```text
//! postcss-discard-duplicates@6.0.0   (Phase 5c)
//! mergeDuplicateAtRules (local)      (Phase 4c)
//! sortAtomicStyleSheet (local)       (Phase 4c)
//! ```
//!
//! When all three of those crates are real, [`sort`] composes them. Until
//! then [`sort`] is an identity passthrough through the postcss-core
//! parser+stringifier so the parity-runner has a fixed surface to drive.

use serde::{Deserialize, Serialize};

use postcss_core::{parse, stringify};

/// Mirrors upstream sort options shape (line 18-26 of `sort.ts`). The
/// upstream `undefined` defaults are *intentional* — they must propagate
/// down to the plugin so its own defaults take effect (see comment on the
/// JS file).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SortOpts {
    #[serde(rename = "sortAtRulesEnabled", default)]
    pub sort_at_rules_enabled: Option<bool>,
    #[serde(rename = "sortShorthandEnabled", default)]
    pub sort_shorthand_enabled: Option<bool>,
}

/// `sort(stylesheet, opts)` — packages/css/src/sort.ts:13. Identity
/// passthrough until Phases 4c + 5c land.
pub fn sort(stylesheet: &str, _opts: &SortOpts) -> String {
    // TODO(phase 4c, 5c): wire postcss-discard-duplicates@6 + local
    // mergeDuplicateAtRules + local sortAtomicStyleSheet, in that order.
    let root = match parse(stylesheet) {
        Ok(r) => r,
        Err(_) => return stylesheet.to_string(),
    };
    stringify(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_round_trips() {
        let css = "a { color: red; }";
        assert_eq!(sort(css, &SortOpts::default()), css);
    }
}
