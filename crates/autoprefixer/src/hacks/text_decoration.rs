//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/text-decoration.js`.
//!
//! ```js
//! let Declaration = require('../declaration')
//!
//! const BASIC = [
//!   'none', 'underline', 'overline', 'line-through', 'blink',
//!   'inherit', 'initial', 'unset'
//! ]
//!
//! class TextDecoration extends Declaration {
//!   /**
//!    * Do not add prefixes for basic values.
//!    */
//!   check(decl) {
//!     return decl.value.split(/\s+/).some(i => !BASIC.includes(i))
//!   }
//! }
//!
//! TextDecoration.names = ['text-decoration']
//! ```
//!
//! Subclass of `Declaration`. Only `check` is overridden — basic
//! single-value `text-decoration: underline` etc. don't need a prefix on
//! AFM Safari, but the shorthand form (`underline solid 2px`) does.

use crate::declaration::DeclarationBase;
use postcss_core::{Node, NodeKind};

const BASIC: &[&str] = &[
    "none",
    "underline",
    "overline",
    "line-through",
    "blink",
    "inherit",
    "initial",
    "unset",
];

#[cfg_attr(feature = "fast-match", derive(serde::Serialize, serde::Deserialize))]
pub struct TextDecoration {
    pub base: DeclarationBase,
}

impl TextDecoration {
    pub const NAMES: &'static [&'static str] = &["text-decoration"];
    pub const CLASS_NAME: &'static str = "TextDecoration";

    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self {
            base: DeclarationBase::new(name, prefixes, all_id),
        }
    }

    /// JS `check(decl)` — split the value on `/\s+/` (one or more
    /// whitespace), return true if ANY token is non-basic.
    /// `decl.value.split(/\s+/)` in JS keeps leading/trailing empty
    /// chunks if the value starts/ends with whitespace; mirrored via
    /// `split_whitespace_keep_empty`.
    pub fn check(&self, decl: &Node) -> bool {
        let value = match &decl.kind {
            NodeKind::Declaration(d) => &d.value,
            _ => return false,
        };
        // JS `String.prototype.split(/\s+/)` returns the run-collapsed
        // splits — trailing empty only if input ends in non-whitespace
        // followed by run of whitespace at the end. For our purpose
        // (`some(i => !BASIC.includes(i))`) an empty token ('') is also
        // not in BASIC, which would change the answer if ANY whitespace
        // run is at the boundary. JS preserves the empty.
        //
        // Practical impact: `text-decoration: underline` → ["underline"]
        // → all basic → check=false. `text-decoration: underline solid`
        // → ["underline", "solid"] → "solid" not basic → check=true.
        // `text-decoration: " underline"` (leading space) → ["", "underline"]
        // → "" not basic → check=true. We replicate this verbatim.
        let parts: Vec<&str> = split_whitespace_keep_empty(value);
        parts.iter().any(|i| !BASIC.contains(i))
    }
}

/// JS `String.prototype.split(/\s+/)`: splits on one-or-more whitespace.
/// Unlike Rust's `str::split_whitespace`, JS preserves empty leading and
/// trailing chunks when the string starts or ends with whitespace.
fn split_whitespace_keep_empty(s: &str) -> Vec<&str> {
    // Mirror JS more faithfully: treat any run of whitespace as one
    // separator, but emit empty strings at the boundaries.
    let mut out: Vec<&str> = Vec::new();
    let mut last = 0usize;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // ASCII whitespace per JS `\s` for our practical inputs (CSS
        // values use ASCII space/tab/newline). Full JS `\s` is broader
        // but values almost never include unicode whitespace.
        if matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c) {
            out.push(&s[last..i]);
            // Skip the whitespace run.
            while i < bytes.len()
                && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
            {
                i += 1;
            }
            last = i;
        } else {
            i += 1;
        }
    }
    out.push(&s[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::parse;

    fn first_decl(root: &mut Node) -> &mut Node {
        let rule = root.nodes_mut().unwrap().get_mut(0).unwrap();
        rule.nodes_mut().unwrap().get_mut(0).unwrap()
    }

    fn td() -> TextDecoration {
        TextDecoration::new("text-decoration".into(), vec!["-webkit-".into()], 0)
    }

    #[test]
    fn check_false_for_single_basic_value() {
        let mut r = parse("a { text-decoration: underline; }").unwrap();
        assert!(!td().check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_false_for_basic_none() {
        let mut r = parse("a { text-decoration: none; }").unwrap();
        assert!(!td().check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_false_for_inherit() {
        let mut r = parse("a { text-decoration: inherit; }").unwrap();
        assert!(!td().check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_true_for_shorthand_with_thickness() {
        let mut r = parse("a { text-decoration: underline solid 2px; }").unwrap();
        // "solid" is non-basic → check=true.
        assert!(td().check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_true_for_shorthand_with_color() {
        let mut r = parse("a { text-decoration: underline red; }").unwrap();
        assert!(td().check(first_decl(&mut r.root)));
    }

    #[test]
    fn check_false_for_two_basic_values() {
        // `underline overline` — both basic, but JS spec: two basic decoration
        // lines is the modern multi-line shorthand. Both are in BASIC, so
        // check=false (no prefix needed).
        let mut r = parse("a { text-decoration: underline overline; }").unwrap();
        assert!(!td().check(first_decl(&mut r.root)));
    }
}
