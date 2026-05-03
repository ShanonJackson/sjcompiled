//! Port of `packages/utils/src/to-boolean.ts`.
//!
//! Upstream `toBoolean<T>(value): value is Exclude<T, Falsy>` simply wraps
//! `Boolean(value)` for use as a `.filter()` predicate. The Rust counterpart
//! is provided as a generic helper for callers iterating `Option`-bearing
//! collections.

/// Returns `true` if the option is `Some(_)`. Equivalent in spirit to JS's
/// `[].filter(toBoolean)` truthy check — `null` / `undefined` become
/// `None` in Rust and are filtered out.
pub fn to_boolean<T>(v: &Option<T>) -> bool { v.is_some() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn some_is_true() {
        assert!(to_boolean(&Some(1)));
        assert!(to_boolean(&Some("")));
    }

    #[test]
    fn none_is_false() {
        let v: Option<i32> = None;
        assert!(!to_boolean(&v));
    }
}
