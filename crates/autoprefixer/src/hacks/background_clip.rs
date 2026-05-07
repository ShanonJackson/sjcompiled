//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/background-clip.js`.
//!
//! ```js
//! let Declaration = require('../declaration')
//! let utils = require('../utils')
//!
//! class BackgroundClip extends Declaration {
//!   constructor(name, prefixes, all) {
//!     super(name, prefixes, all)
//!
//!     if (this.prefixes) {
//!       this.prefixes = utils.uniq(
//!         this.prefixes.map(i => {
//!           return i === '-ms-' ? '-webkit-' : i
//!         })
//!       )
//!     }
//!   }
//!
//!   check(decl) {
//!     return decl.value.toLowerCase() === 'text'
//!   }
//! }
//!
//! BackgroundClip.names = ['background-clip']
//! ```
//!
//! Subclass of `Declaration`. Two overrides:
//!
//! 1. Constructor rewrites `-ms-` → `-webkit-` in the prefix list and
//!    de-dupes (Edge/IE-era `-ms-background-clip` was a no-op, so the
//!    only real prefix that ever needs emitting is `-webkit-`).
//! 2. `check(decl)` gates emission on `decl.value === 'text'` —
//!    `-webkit-background-clip: text` is the long-standing webkit
//!    extension for clipping a background image to text glyphs. Every
//!    other value (`content-box`, `padding-box`, `border-box`, `initial`,
//!    `inherit`, …) is unprefixed in every shipping browser, so adding
//!    a `-webkit-` prefix would just bloat output bytes.
//!
//! Without this hack, `background-clip: content-box` (and friends) would
//! pick up a spurious `-webkit-background-clip: content-box` clone on
//! any browserslist that ships a `-webkit-` entry for `background-clip`
//! in the prefix data tables — including modern targets like
//! `chrome 100`. AFM corpus exercises this on every `background-clip`
//! declaration emitted by `@atlaskit/*` style props (Group A in
//! `parity-runner/corpus/afm-transform-css/AFM_TRIAGE.md`).

use crate::declaration::DeclarationBase;
use crate::utils::uniq;
use postcss_core::{Node, NodeKind};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct BackgroundClip {
    pub base: DeclarationBase,
}

impl BackgroundClip {
    pub const NAMES: &'static [&'static str] = &["background-clip"];
    pub const CLASS_NAME: &'static str = "BackgroundClip";

    /// JS constructor: rewrite `-ms-` → `-webkit-` in the prefix list,
    /// then `utils.uniq`. Mirrors `lib/hacks/background-clip.js:5-15`.
    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        let rewritten: Vec<String> = prefixes
            .into_iter()
            .map(|p| if p == "-ms-" { "-webkit-".to_string() } else { p })
            .collect();
        let unique = uniq(&rewritten);
        Self {
            base: DeclarationBase::new(name, unique, all_id),
        }
    }

    /// JS `check(decl)` — `decl.value.toLowerCase() === 'text'`. The
    /// only value that needs `-webkit-background-clip` on shipping
    /// browsers; everything else (initial keyword set, the four `*-box`
    /// values) ships unprefixed.
    ///
    /// Mirrors `lib/hacks/background-clip.js:17-19`.
    pub fn check(&self, decl: &Node) -> bool {
        let value = match &decl.kind {
            NodeKind::Declaration(d) => &d.value,
            _ => return false,
        };
        value.to_lowercase() == "text"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::parse;

    fn first_decl(root: &mut Node) -> &mut Node {
        let rule = root.nodes_mut().unwrap().get_mut(0).unwrap();
        rule.nodes_mut().unwrap().get_mut(0).unwrap()
    }

    fn bc_with(prefixes: Vec<&str>) -> BackgroundClip {
        BackgroundClip::new(
            "background-clip".to_string(),
            prefixes.into_iter().map(String::from).collect(),
            0,
        )
    }

    #[test]
    fn check_true_for_text_value() {
        let mut r = parse("a { background-clip: text; }").unwrap();
        assert!(bc_with(vec!["-webkit-"]).check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_true_for_text_value_uppercase() {
        // JS `decl.value.toLowerCase() === 'text'` is case-insensitive.
        let mut r = parse("a { background-clip: TEXT; }").unwrap();
        assert!(bc_with(vec!["-webkit-"]).check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_false_for_content_box() {
        let mut r = parse("a { background-clip: content-box; }").unwrap();
        assert!(!bc_with(vec!["-webkit-"]).check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_false_for_padding_box() {
        let mut r = parse("a { background-clip: padding-box; }").unwrap();
        assert!(!bc_with(vec!["-webkit-"]).check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_false_for_border_box() {
        let mut r = parse("a { background-clip: border-box; }").unwrap();
        assert!(!bc_with(vec!["-webkit-"]).check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_false_for_initial_keyword() {
        // `background-clip: initial` — the AFM corpus's reduce-initial
        // output. Must NOT trigger the webkit prefix.
        let mut r = parse("a { background-clip: initial; }").unwrap();
        assert!(!bc_with(vec!["-webkit-"]).check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_false_for_var_call() {
        // `background-clip: var(--x)` — AFM corpus also emits these.
        let mut r = parse("a { background-clip: var(--x); }").unwrap();
        assert!(!bc_with(vec!["-webkit-"]).check(first_decl(&mut r.root)));
    }

    #[test]
    fn ctor_rewrites_ms_to_webkit() {
        let h = bc_with(vec!["-ms-"]);
        assert_eq!(h.base.prefixer.prefixes, vec!["-webkit-".to_string()]);
    }

    #[test]
    fn ctor_dedupes_webkit_after_ms_rewrite() {
        // `-ms-` → `-webkit-`, then uniq — only one `-webkit-` survives.
        let h = bc_with(vec!["-webkit-", "-ms-"]);
        assert_eq!(h.base.prefixer.prefixes, vec!["-webkit-".to_string()]);
    }

    #[test]
    fn ctor_preserves_other_prefixes_in_order() {
        // Non-`-ms-` prefixes pass through unchanged. Order preserved
        // (uniq keeps first occurrence). `-moz-` isn't in the
        // background-clip prefix data tables in practice, but the
        // constructor's rewrite logic doesn't filter on that — it only
        // touches `-ms-`.
        let h = bc_with(vec!["-webkit-", "-moz-"]);
        assert_eq!(
            h.base.prefixer.prefixes,
            vec!["-webkit-".to_string(), "-moz-".to_string()]
        );
    }

    #[test]
    fn ctor_empty_prefix_list_yields_empty() {
        let h = bc_with(vec![]);
        assert!(h.base.prefixer.prefixes.is_empty());
    }
}
