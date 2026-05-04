//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/user-select.js`.
//!
//! ```js
//! let Declaration = require('../declaration')
//!
//! class UserSelect extends Declaration {
//!   /**
//!    * Change prefixed value for IE
//!    */
//!   set(decl, prefix) {
//!     if (prefix === '-ms-' && decl.value === 'contain') {
//!       decl.value = 'element'
//!     }
//!     return super.set(decl, prefix)
//!   }
//!
//!   /**
//!    * Avoid prefixing all in IE
//!    */
//!   insert(decl, prefix, prefixes) {
//!     if (decl.value === 'all' && prefix === '-ms-') {
//!       return undefined
//!     } else {
//!       return super.insert(decl, prefix, prefixes)
//!     }
//!   }
//! }
//!
//! UserSelect.names = ['user-select']
//! ```
//!
//! Subclass of `Declaration`. Two overrides:
//! 1. `set` — `-ms-` + `value === 'contain'` rewrites the value to
//!    `'element'` (legacy IE used `element` instead of `contain`).
//! 2. `insert` — `-ms-` + `value === 'all'` skips the prefix entirely
//!    (IE doesn't support `user-select: all`).
//!
//! Both -ms- branches are no-ops for the AFM browserslist (no IE
//! targeted), but must be ported byte-faithfully so the hack matches
//! upstream on inputs that hypothetically would.

use crate::declaration::DeclarationBase;
use postcss_core::{Node, NodeKind};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct UserSelect {
    pub base: DeclarationBase,
}

impl UserSelect {
    pub const NAMES: &'static [&'static str] = &["user-select"];
    pub const CLASS_NAME: &'static str = "UserSelect";

    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self {
            base: DeclarationBase::new(name, prefixes, all_id),
        }
    }

    /// JS `set(decl, prefix)`. The `-ms-` + `contain` branch mutates
    /// `decl.value` BEFORE delegating to `super.set` — i.e. the value
    /// rewrite persists onto the cloned node.
    pub fn set(&self, decl: &mut Node, prefix: &str) -> Option<()> {
        if prefix == "-ms-" {
            if let NodeKind::Declaration(d) = &mut decl.kind {
                if d.value == "contain" {
                    d.value = "element".to_string();
                }
            }
        }
        self.base.set(decl, prefix)
    }

    /// JS `insert(decl, prefix, prefixes)`. The `-ms-` + `all` branch
    /// returns `undefined` (= no clone inserted, no work done).
    /// Otherwise delegates to `super.insert`.
    pub fn insert(
        &self,
        root: &mut Node,
        path: &[usize],
        prefix: &str,
        prefixes: &[String],
    ) -> Option<()> {
        // Read the value before delegating; signature forces `path` use.
        let value_is_all = match postcss_core::node_at_path(root, path) {
            Some(n) => match &n.kind {
                NodeKind::Declaration(d) => d.value == "all",
                _ => return None,
            },
            None => return None,
        };
        if value_is_all && prefix == "-ms-" {
            return None;
        }
        // For `set` to fire the `-ms-/contain` rename on the clone,
        // `DeclarationBase::insert` would need to dispatch to OUR `set`.
        // The base instead calls `self.set(...)` on the base struct
        // directly. That's a Rust-side coupling we can't fix without
        // changing the trait, but it's only material for `-ms-` (NOT in
        // AFM scope). For AFM (`-webkit-` only), `super.insert` is
        // equivalent. File this in handover for AGENT_4 to thread the
        // right `set` through dispatch when `-ms-` ever matters.
        self.base.insert(root, path, prefix, prefixes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn first_decl(root: &mut Node) -> &mut Node {
        let rule = root.nodes_mut().unwrap().get_mut(0).unwrap();
        rule.nodes_mut().unwrap().get_mut(0).unwrap()
    }

    fn us() -> UserSelect {
        UserSelect::new("user-select".into(), vec!["-webkit-".into()], 0)
    }

    #[test]
    fn set_does_not_rewrite_value_for_webkit() {
        let mut r = parse("a { user-select: contain; }").unwrap();
        us().set(first_decl(&mut r.root), "-webkit-");
        match &first_decl(&mut r.root).kind {
            NodeKind::Declaration(d) => {
                assert_eq!(d.prop, "-webkit-user-select");
                // Value untouched — only -ms- triggers the rename.
                assert_eq!(d.value, "contain");
            }
            _ => panic!("expected decl"),
        }
    }

    #[test]
    fn set_rewrites_contain_to_element_for_ms() {
        let mut r = parse("a { user-select: contain; }").unwrap();
        us().set(first_decl(&mut r.root), "-ms-");
        match &first_decl(&mut r.root).kind {
            NodeKind::Declaration(d) => {
                assert_eq!(d.prop, "-ms-user-select");
                assert_eq!(d.value, "element");
            }
            _ => panic!("expected decl"),
        }
    }

    #[test]
    fn set_unchanged_for_non_contain_value_under_ms() {
        let mut r = parse("a { user-select: none; }").unwrap();
        us().set(first_decl(&mut r.root), "-ms-");
        match &first_decl(&mut r.root).kind {
            NodeKind::Declaration(d) => {
                assert_eq!(d.prop, "-ms-user-select");
                assert_eq!(d.value, "none");
            }
            _ => panic!("expected decl"),
        }
    }

    #[test]
    fn insert_skips_all_for_ms_prefix() {
        let mut r = parse("a { user-select: all; }").unwrap();
        let len_before = r.root.nodes().unwrap()[0].nodes().unwrap().len();
        us().insert(&mut r.root, &[0, 0], "-ms-", &["-ms-".into()]);
        let len_after = r.root.nodes().unwrap()[0].nodes().unwrap().len();
        assert_eq!(len_before, len_after, "no clone should have been inserted");
    }

    #[test]
    fn insert_proceeds_for_all_under_webkit() {
        let mut r = parse("a { user-select: all; }").unwrap();
        let len_before = r.root.nodes().unwrap()[0].nodes().unwrap().len();
        us().insert(
            &mut r.root,
            &[0, 0],
            "-webkit-",
            &["-webkit-".into()],
        );
        let len_after = r.root.nodes().unwrap()[0].nodes().unwrap().len();
        assert_eq!(
            len_after,
            len_before + 1,
            "clone should have been inserted before original"
        );
        let out = stringify(&r);
        assert!(out.contains("-webkit-user-select: all"));
    }

    #[test]
    fn insert_proceeds_for_non_all_value_under_ms() {
        let mut r = parse("a { user-select: none; }").unwrap();
        let len_before = r.root.nodes().unwrap()[0].nodes().unwrap().len();
        us().insert(&mut r.root, &[0, 0], "-ms-", &["-ms-".into()]);
        let len_after = r.root.nodes().unwrap()[0].nodes().unwrap().len();
        assert_eq!(len_after, len_before + 1);
    }
}
