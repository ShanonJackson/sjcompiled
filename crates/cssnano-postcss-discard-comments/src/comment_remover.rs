//! Port of `node_modules/postcss-discard-comments@5.1.2/src/lib/commentRemover.js`.
//!
//! Folder-mapping deviation: upstream lives at `src/lib/commentRemover.js`,
//! ported to `src/comment_remover.rs` (see `comment_parser.rs` header for
//! the Rust crate-root naming reason).
//!
//! Upstream is a tiny stateful predicate: holds the plugin options +
//! a `_hasFirst` boolean used by `removeAllButFirst`.

use crate::DiscardCommentsOpts;

/// Stateful comment-removal predicate. `_hasFirst` flips to `true`
/// the first time we encounter an `/*!` important comment under
/// `removeAllButFirst` mode.
pub struct CommentRemover<'o> {
    pub options: &'o DiscardCommentsOpts,
    has_first: bool,
}

impl<'o> CommentRemover<'o> {
    pub fn new(options: &'o DiscardCommentsOpts) -> Self {
        CommentRemover { options, has_first: false }
    }

    /// Mirrors upstream `canRemove(comment)`. Returns `Some(true)` to
    /// remove, `Some(false)` to keep, `None` for upstream's `undefined`
    /// fall-through (which JS treats as falsy → keep).
    ///
    /// `comment` is the raw body BETWEEN `/*` and `*/` (no delimiters).
    /// The leading `!` test is upstream's `comment.indexOf('!') === 0`.
    pub fn can_remove(&mut self, comment: &str) -> Option<bool> {
        if let Some(predicate) = &self.options.remove {
            return Some(predicate(comment));
        }
        let is_important = comment.starts_with('!');
        if !is_important {
            return Some(true);
        }
        if self.options.remove_all || self.has_first {
            return Some(true);
        }
        if self.options.remove_all_but_first && !self.has_first {
            self.has_first = true;
            return Some(false);
        }
        // Upstream falls through to `undefined`. JS treats this as falsy
        // (don't remove), which matches the default-options behavior of
        // keeping `/*!` comments.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> DiscardCommentsOpts {
        DiscardCommentsOpts::default()
    }

    #[test]
    fn default_removes_non_important() {
        let mut o = opts();
        let mut r = CommentRemover::new(&o);
        assert_eq!(r.can_remove(" foo "), Some(true));
        let _ = &mut o; // keep `o` alive borrow guard
    }

    #[test]
    fn default_keeps_important() {
        let o = opts();
        let mut r = CommentRemover::new(&o);
        // `!` at position 0 → important → undefined (None) → keep.
        assert_eq!(r.can_remove("! foo"), None);
    }

    #[test]
    fn remove_all_drops_important() {
        let mut o = opts();
        o.remove_all = true;
        let mut r = CommentRemover::new(&o);
        assert_eq!(r.can_remove("! foo"), Some(true));
    }

    #[test]
    fn remove_all_but_first_keeps_first_important_only() {
        let mut o = opts();
        o.remove_all_but_first = true;
        let mut r = CommentRemover::new(&o);
        assert_eq!(r.can_remove("! foo"), Some(false));
        assert_eq!(r.can_remove("! bar"), Some(true));
    }
}
