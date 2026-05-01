//! Port of `cssnano-utils/src/sameParent.js`.
//!
//! Upstream walks up `nodeA.parent` and `nodeB.parent`, comparing types and
//! (for `atrule`) the params/name. Our AST holds parent links externally,
//! so the port takes a small `ParentRef` view.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeShape<'a> {
    Rule { kind: &'static str },
    AtRule { name: &'a str, params: &'a str },
    Other { kind: &'static str },
}

/// `checkMatch(nodeA, nodeB)` (upstream lines 8-16).
pub fn check_match(a: &NodeShape, b: &NodeShape) -> bool {
    match (a, b) {
        (NodeShape::AtRule { name: na, params: pa }, NodeShape::AtRule { name: nb, params: pb }) => {
            pa == pb && na.to_lowercase() == nb.to_lowercase()
        }
        (NodeShape::Rule { kind: ka }, NodeShape::Rule { kind: kb }) => ka == kb,
        (NodeShape::Other { kind: ka }, NodeShape::Other { kind: kb }) => ka == kb,
        _ => false,
    }
}

/// `sameParent(nodeA, nodeB)` (upstream lines 24-42).
pub fn same_parent(a_chain: &[NodeShape], b_chain: &[NodeShape]) -> bool {
    if a_chain.is_empty() { return b_chain.is_empty(); }
    if b_chain.is_empty() { return false; }
    if !check_match(&a_chain[0], &b_chain[0]) { return false; }
    same_parent(&a_chain[1..], &b_chain[1..])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_rule_chains() {
        let a = vec![NodeShape::Rule { kind: "rule" }];
        let b = vec![NodeShape::Rule { kind: "rule" }];
        assert!(same_parent(&a, &b));
    }
    #[test]
    fn rejects_atrule_param_diff() {
        let a = vec![NodeShape::AtRule { name: "media", params: "(max-width: 100px)" }];
        let b = vec![NodeShape::AtRule { name: "media", params: "(max-width: 200px)" }];
        assert!(!same_parent(&a, &b));
    }
    #[test]
    fn case_insensitive_atrule_name() {
        let a = vec![NodeShape::AtRule { name: "MEDIA", params: "x" }];
        let b = vec![NodeShape::AtRule { name: "media", params: "x" }];
        assert!(same_parent(&a, &b));
    }
}
