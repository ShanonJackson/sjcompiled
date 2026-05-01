//! Port of `cssnano-utils/src/getArguments.js`.
//!
//! Upstream signature: `getArguments(node) -> Node[][]`.
//! Splits a `postcss-value-parser` Function node's children at top-level
//! `div` (comma) tokens.

/// Generic split-by-divider helper. The caller passes `is_div(child)` to
/// avoid coupling this crate to a specific value-parser flavour.
pub fn get_arguments<T: Clone, F: Fn(&T) -> bool>(nodes: &[T], is_div: F) -> Vec<Vec<T>> {
    let mut list: Vec<Vec<T>> = vec![Vec::new()];
    for child in nodes {
        if !is_div(child) {
            list.last_mut().unwrap().push(child.clone());
        } else {
            list.push(Vec::new());
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn splits_on_div() {
        let v = vec!["a", ",", "b", ",", "c"];
        let out = get_arguments(&v, |c| *c == ",");
        assert_eq!(out, vec![vec!["a"], vec!["b"], vec!["c"]]);
    }
    #[test]
    fn empty() {
        let v: Vec<&str> = vec![];
        let out = get_arguments(&v, |c| *c == ",");
        assert_eq!(out, vec![Vec::<&str>::new()]);
    }
}
