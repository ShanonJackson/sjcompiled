//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/intrinsic.js`.
//!
//! ```js
//! let OldValue = require('../old-value')
//! let Value = require('../value')
//!
//! function regexp(name) {
//!   return new RegExp(`(^|[\\s,(])(${name}($|[\\s),]))`, 'gi')
//! }
//!
//! class Intrinsic extends Value {
//!   regexp() {
//!     if (!this.regexpCache) this.regexpCache = regexp(this.name)
//!     return this.regexpCache
//!   }
//!
//!   isStretch() {
//!     return (
//!       this.name === 'stretch' ||
//!       this.name === 'fill' ||
//!       this.name === 'fill-available'
//!     )
//!   }
//!
//!   replace(string, prefix) {
//!     if (prefix === '-moz-' && this.isStretch()) {
//!       return string.replace(this.regexp(), '$1-moz-available$3')
//!     }
//!     if (prefix === '-webkit-' && this.isStretch()) {
//!       return string.replace(this.regexp(), '$1-webkit-fill-available$3')
//!     }
//!     return super.replace(string, prefix)
//!   }
//!
//!   old(prefix) {
//!     let prefixed = prefix + this.name
//!     if (this.isStretch()) {
//!       if (prefix === '-moz-') prefixed = '-moz-available'
//!       else if (prefix === '-webkit-') prefixed = '-webkit-fill-available'
//!     }
//!     return new OldValue(this.name, prefixed, prefixed, regexp(prefixed))
//!   }
//!
//!   add(decl, prefix) {
//!     if (decl.prop.includes('grid') && prefix !== '-webkit-') {
//!       return undefined
//!     }
//!     return super.add(decl, prefix)
//!   }
//! }
//!
//! Intrinsic.names = ['max-content', 'min-content', 'fit-content',
//!                    'fill', 'fill-available', 'stretch']
//! ```
//!
//! **Subtle byte-equality risk** — Intrinsic builds its own regexp via
//! the local `regexp(name)` function, NOT the shared `utils.regexp`. The
//! third character class differs by one character: Intrinsic uses
//! `($|[\s),])` (closing paren), `utils.regexp` uses `($|[\s(,])`
//! (opening paren). Mirroring this is load-bearing — without the
//! Intrinsic-local form, `width: max(fit-content, 100px)` would not
//! match the trailing `,` boundary.

use crate::old_value::OldValue;
use crate::utils;
use crate::value::ValueBase;
use once_cell::sync::OnceCell;
use postcss_core::{Node, NodeKind};
use regex::Regex;

/// Matches `name` with the Intrinsic-local trailing boundary `[\s),]`.
fn intrinsic_regexp(name: &str) -> Regex {
    let escaped = utils::escape_regexp(name);
    let src = format!("(^|[\\s,(])({}($|[\\s),]))", escaped);
    Regex::new(&format!("(?i){}", src)).expect("valid intrinsic regexp")
}

pub struct Intrinsic {
    pub base: ValueBase,
    /// JS lazy `this.regexpCache`. Distinct from `ValueBase`'s own cache
    /// because the regex source differs.
    regexp_cache: OnceCell<Regex>,
}

impl Intrinsic {
    pub const NAMES: &'static [&'static str] = &[
        "max-content",
        "min-content",
        "fit-content",
        "fill",
        "fill-available",
        "stretch",
    ];
    pub const CLASS_NAME: &'static str = "Intrinsic";

    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self {
            base: ValueBase::new(name, prefixes, all_id),
            regexp_cache: OnceCell::new(),
        }
    }

    /// JS `regexp()` — overrides `ValueBase::regexp` to use the local
    /// `regexp(name)` form. Lazy.
    pub fn regexp(&self) -> &Regex {
        self.regexp_cache
            .get_or_init(|| intrinsic_regexp(&self.base.prefixer.name))
    }

    /// JS `isStretch()` — only `stretch` / `fill` / `fill-available` get
    /// the alias-rename treatment in `replace` and `old`. The other three
    /// names (`max-content` / `min-content` / `fit-content`) fall through
    /// to base behaviour (`<prefix><name>`).
    pub fn is_stretch(&self) -> bool {
        let n = self.base.prefixer.name.as_str();
        n == "stretch" || n == "fill" || n == "fill-available"
    }

    /// JS `replace(string, prefix)`. For stretch-family + (`-moz-` |
    /// `-webkit-`), substitute a vendor-specific alias name; otherwise
    /// fall through to `ValueBase::replace`.
    ///
    /// JS replacement strings `'$1-moz-available$3'` / `'$1-webkit-fill-available$3'`
    /// reference capture groups 1 and 3 from the Intrinsic-local regex.
    /// Group 1 = leading boundary (`^` / `[\s,(]`), group 3 = trailing
    /// boundary (`$` / `[\s),]`). Group 2 (the matched name) is dropped.
    pub fn replace(&self, string: &str, prefix: &str) -> String {
        if (prefix == "-moz-" || prefix == "-webkit-") && self.is_stretch() {
            let alias = if prefix == "-moz-" {
                "-moz-available"
            } else {
                "-webkit-fill-available"
            };
            return self
                .regexp()
                .replace_all(string, |caps: &regex::Captures| {
                    let g1 = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    let g3 = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                    format!("{g1}{alias}{g3}")
                })
                .into_owned();
        }
        // Base `Value.replace` — but JS Intrinsic still uses its OWN
        // regexp via `super.replace` (super looks up `this.regexp()`,
        // which is the override). So we must call our own regex too.
        self.regexp()
            .replace_all(string, |caps: &regex::Captures| {
                let g1 = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let g2 = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                format!("{g1}{prefix}{g2}")
            })
            .into_owned()
    }

    /// JS `old(prefix)` — returns an `OldValue` pre-configured with the
    /// vendor-aliased prefixed string AND the local-regex-source-derived
    /// regex (NOT `utils.regexp(prefixed)`).
    pub fn old(&self, prefix: &str) -> OldValue {
        let mut prefixed = format!("{prefix}{}", self.base.prefixer.name);
        if self.is_stretch() {
            if prefix == "-moz-" {
                prefixed = "-moz-available".to_string();
            } else if prefix == "-webkit-" {
                prefixed = "-webkit-fill-available".to_string();
            }
        }
        OldValue::new(
            self.base.prefixer.name.clone(),
            prefixed.clone(),
            Some(prefixed.clone()),
            Some(intrinsic_regexp(&prefixed)),
        )
    }

    /// JS `add(decl, prefix)`. Skip when the decl is on a `grid-*`
    /// property and the prefix isn't `-webkit-` (other prefixes don't
    /// emit IE-style grid value renames). Otherwise delegate to
    /// `ValueBase::add`.
    pub fn add(&mut self, decl: &mut Node, prefix: &str) {
        let prop_has_grid = match &decl.kind {
            NodeKind::Declaration(d) => d.prop.contains("grid"),
            _ => false,
        };
        if prop_has_grid && prefix != "-webkit-" {
            return;
        }
        // ValueBase::add itself uses `self.regexp()` indirectly via
        // `self.replace(...)`. Since `add` here calls `super.add` which
        // calls `this.replace` — which IS our override — we re-implement
        // the loop locally to keep the override hooked.
        let initial = self
            .stored_value(decl, prefix)
            .unwrap_or_else(|| self.base.value(decl));

        let mut value = initial;
        loop {
            let before = value.clone();
            value = self.replace(&before, prefix);
            if value == before {
                break;
            }
        }

        let map = decl
            .attrs
            .get_string_map_mut(crate::value::ATTR_VALUES);
        match map {
            Some(m) => {
                m.insert(prefix.to_string(), value);
            }
            None => {
                let mut m = indexmap::IndexMap::new();
                m.insert(prefix.to_string(), value);
                decl.attrs
                    .set(crate::value::ATTR_VALUES, postcss_core::AttrValue::StringMap(m));
            }
        }
    }

    fn stored_value(&self, decl: &Node, prefix: &str) -> Option<String> {
        decl.attrs
            .get_string_map(crate::value::ATTR_VALUES)
            .and_then(|m| m.get(prefix).cloned())
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

    #[test]
    fn is_stretch_recognises_three_names() {
        let h = Intrinsic::new("stretch".into(), vec![], 0);
        assert!(h.is_stretch());
        let h = Intrinsic::new("fill".into(), vec![], 0);
        assert!(h.is_stretch());
        let h = Intrinsic::new("fill-available".into(), vec![], 0);
        assert!(h.is_stretch());
        let h = Intrinsic::new("fit-content".into(), vec![], 0);
        assert!(!h.is_stretch());
        let h = Intrinsic::new("min-content".into(), vec![], 0);
        assert!(!h.is_stretch());
    }

    #[test]
    fn replace_stretch_to_moz_available() {
        let h = Intrinsic::new("stretch".into(), vec!["-moz-".into()], 0);
        assert_eq!(h.replace("stretch", "-moz-"), "-moz-available");
    }

    #[test]
    fn replace_stretch_to_webkit_fill_available() {
        let h = Intrinsic::new("stretch".into(), vec!["-webkit-".into()], 0);
        assert_eq!(h.replace("stretch", "-webkit-"), "-webkit-fill-available");
    }

    #[test]
    fn replace_fit_content_passes_through_to_base() {
        // Non-stretch name → falls through to base prefix concat.
        let h = Intrinsic::new("fit-content".into(), vec!["-moz-".into()], 0);
        assert_eq!(h.replace("fit-content", "-moz-"), "-moz-fit-content");
    }

    #[test]
    fn replace_fill_to_webkit_fill_available() {
        let h = Intrinsic::new("fill".into(), vec!["-webkit-".into()], 0);
        assert_eq!(h.replace("fill", "-webkit-"), "-webkit-fill-available");
    }

    #[test]
    fn replace_preserves_leading_boundary() {
        // Leading `(` (group 1 captures `[\s,(]`).
        let h = Intrinsic::new("fit-content".into(), vec!["-moz-".into()], 0);
        assert_eq!(
            h.replace("calc(fit-content)", "-moz-"),
            "calc(-moz-fit-content)"
        );
    }

    #[test]
    fn old_stretch_uses_alias_for_moz() {
        let h = Intrinsic::new("stretch".into(), vec!["-moz-".into()], 0);
        let ov = h.old("-moz-");
        assert_eq!(ov.unprefixed, "stretch");
        assert_eq!(ov.prefixed, "-moz-available");
    }

    #[test]
    fn old_stretch_uses_alias_for_webkit() {
        let h = Intrinsic::new("fill".into(), vec!["-webkit-".into()], 0);
        let ov = h.old("-webkit-");
        assert_eq!(ov.prefixed, "-webkit-fill-available");
    }

    #[test]
    fn old_non_stretch_uses_concat() {
        let h = Intrinsic::new("fit-content".into(), vec!["-moz-".into()], 0);
        let ov = h.old("-moz-");
        assert_eq!(ov.prefixed, "-moz-fit-content");
    }

    #[test]
    fn add_skips_grid_prop_when_prefix_not_webkit() {
        let mut r = parse("a { grid-template-columns: fit-content; }").unwrap();
        let mut h = Intrinsic::new("fit-content".into(), vec!["-moz-".into()], 0);
        h.add(first_decl(&mut r.root), "-moz-");
        // No `_autoprefixerValues` map should be created.
        assert!(first_decl(&mut r.root)
            .attrs
            .get_string_map(crate::value::ATTR_VALUES)
            .is_none());
    }

    #[test]
    fn add_caches_prefixed_value_for_non_grid_prop() {
        let mut r = parse("a { width: fit-content; }").unwrap();
        let mut h = Intrinsic::new("fit-content".into(), vec!["-moz-".into()], 0);
        h.add(first_decl(&mut r.root), "-moz-");
        let cached = first_decl(&mut r.root)
            .attrs
            .get_string_map(crate::value::ATTR_VALUES)
            .unwrap();
        assert_eq!(cached.get("-moz-").unwrap(), "-moz-fit-content");
    }

    #[test]
    fn add_stretch_emits_alias_into_cache() {
        let mut r = parse("a { width: stretch; }").unwrap();
        let mut h = Intrinsic::new("stretch".into(), vec!["-webkit-".into()], 0);
        h.add(first_decl(&mut r.root), "-webkit-");
        let cached = first_decl(&mut r.root)
            .attrs
            .get_string_map(crate::value::ATTR_VALUES)
            .unwrap();
        assert_eq!(cached.get("-webkit-").unwrap(), "-webkit-fill-available");
    }

    #[test]
    fn add_grid_prop_allowed_for_webkit() {
        let mut r = parse("a { grid-template-columns: fit-content; }").unwrap();
        let mut h = Intrinsic::new("fit-content".into(), vec!["-webkit-".into()], 0);
        h.add(first_decl(&mut r.root), "-webkit-");
        let cached = first_decl(&mut r.root)
            .attrs
            .get_string_map(crate::value::ATTR_VALUES)
            .unwrap();
        // `-webkit-fit-content` (super.replace path; not an alias).
        assert_eq!(cached.get("-webkit-").unwrap(), "-webkit-fit-content");
    }
}
