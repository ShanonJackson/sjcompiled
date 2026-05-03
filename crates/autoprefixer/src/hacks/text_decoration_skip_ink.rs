//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/text-decoration-skip-ink.js`.
//!
//! ```js
//! let Declaration = require('../declaration')
//!
//! class TextDecorationSkipInk extends Declaration {
//!   /**
//!    * Change prefix for ink value
//!    */
//!   set(decl, prefix) {
//!     if (decl.prop === 'text-decoration-skip-ink' && decl.value === 'auto') {
//!       decl.prop = prefix + 'text-decoration-skip'
//!       decl.value = 'ink'
//!       return decl
//!     } else {
//!       return super.set(decl, prefix)
//!     }
//!   }
//! }
//!
//! TextDecorationSkipInk.names = ['text-decoration-skip-ink', 'text-decoration-skip']
//! ```
//!
//! Subclass of `Declaration`. Only `set` is overridden — the
//! `text-decoration-skip-ink: auto` modern form is rewritten to
//! `<prefix>text-decoration-skip: ink` (the legacy WebKit syntax) when
//! emitting the prefixed clone. All other prop/value combinations fall
//! through to the base `Declaration.set` (= `decl.prop = prefix + decl.prop`).

use crate::declaration::DeclarationBase;
use postcss_core::{Node, NodeKind};

pub struct TextDecorationSkipInk {
    pub base: DeclarationBase,
}

impl TextDecorationSkipInk {
    pub const NAMES: &'static [&'static str] =
        &["text-decoration-skip-ink", "text-decoration-skip"];
    pub const CLASS_NAME: &'static str = "TextDecorationSkipInk";

    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self {
            base: DeclarationBase::new(name, prefixes, all_id),
        }
    }

    /// JS `set(decl, prefix)`. Only the `text-decoration-skip-ink: auto`
    /// case triggers the rename; all other prop/value combinations
    /// (incl. `text-decoration-skip: ink` already in legacy form) fall
    /// through to `DeclarationBase::set`.
    pub fn set(&self, decl: &mut Node, prefix: &str) -> Option<()> {
        let (prop_is_modern, val_is_auto) = match &decl.kind {
            NodeKind::Declaration(d) => {
                (d.prop == "text-decoration-skip-ink", d.value == "auto")
            }
            _ => return None,
        };
        if prop_is_modern && val_is_auto {
            if let NodeKind::Declaration(d) = &mut decl.kind {
                d.prop = format!("{prefix}text-decoration-skip");
                d.value = "ink".to_string();
            }
            return Some(());
        }
        self.base.set(decl, prefix)
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

    fn h() -> TextDecorationSkipInk {
        TextDecorationSkipInk::new(
            "text-decoration-skip-ink".into(),
            vec!["-webkit-".into()],
            0,
        )
    }

    #[test]
    fn set_renames_skip_ink_auto_to_skip_ink() {
        let mut r = parse("a { text-decoration-skip-ink: auto; }").unwrap();
        h().set(first_decl(&mut r.root), "-webkit-");
        match &first_decl(&mut r.root).kind {
            NodeKind::Declaration(d) => {
                assert_eq!(d.prop, "-webkit-text-decoration-skip");
                assert_eq!(d.value, "ink");
            }
            _ => panic!("expected decl"),
        }
    }

    #[test]
    fn set_falls_through_to_base_for_non_auto_value() {
        let mut r = parse("a { text-decoration-skip-ink: none; }").unwrap();
        h().set(first_decl(&mut r.root), "-webkit-");
        match &first_decl(&mut r.root).kind {
            NodeKind::Declaration(d) => {
                // Base set: `prop = prefix + prop`, value untouched.
                assert_eq!(d.prop, "-webkit-text-decoration-skip-ink");
                assert_eq!(d.value, "none");
            }
            _ => panic!("expected decl"),
        }
    }

    #[test]
    fn set_falls_through_for_text_decoration_skip_prop() {
        let mut r = parse("a { text-decoration-skip: ink; }").unwrap();
        let h = TextDecorationSkipInk::new(
            "text-decoration-skip".into(),
            vec!["-webkit-".into()],
            0,
        );
        h.set(first_decl(&mut r.root), "-webkit-");
        match &first_decl(&mut r.root).kind {
            NodeKind::Declaration(d) => {
                assert_eq!(d.prop, "-webkit-text-decoration-skip");
                assert_eq!(d.value, "ink");
            }
            _ => panic!("expected decl"),
        }
    }
}
