//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/at-rule.js`.
//!
//! `class AtRule extends Prefixer` — only `add` and `process`.

use postcss_core::{insert_before_at_path, parent_some, NodeKind};

use crate::prefixer::{clone_node, parent_prefix_cached_mut, ParentPrefix, PrefixerBase};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AtRuleBase {
    pub prefixer: PrefixerBase,
}

impl AtRuleBase {
    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self { prefixer: PrefixerBase::new(name, prefixes, all_id) }
    }

    /// JS: `add(rule, prefix)`.
    /// ```js
    /// add(rule, prefix) {
    ///   let prefixed = prefix + rule.name
    ///   let already = rule.parent.some(
    ///     i => i.name === prefixed && i.params === rule.params
    ///   )
    ///   if (already) return undefined
    ///   let cloned = this.clone(rule, { name: prefixed })
    ///   return rule.parent.insertBefore(rule, cloned)
    /// }
    /// ```
    pub fn add(
        &mut self,
        root: &mut postcss_core::Node,
        path: &[usize],
        prefix: &str,
    ) -> Option<()> {
        // Read this rule's name + params under an immutable borrow first.
        let (name, params) = {
            let here = postcss_core::node_at_path(root, path)?;
            match &here.kind {
                NodeKind::AtRule(at) => (at.name.clone(), at.params.clone()),
                _ => return None,
            }
        };
        let prefixed = format!("{prefix}{name}");

        // `rule.parent.some(...)` — already a sibling with the prefixed
        // name + same params?
        let already = parent_some(root, path, |sibling| match &sibling.kind {
            NodeKind::AtRule(s) => s.name == prefixed && s.params == params,
            _ => false,
        });
        if already {
            return None;
        }

        // `this.clone(rule, { name: prefixed })`
        let original = postcss_core::node_at_path(root, path)?;
        let mut cloned = clone_node(original);
        if let NodeKind::AtRule(ref mut at) = cloned.kind {
            at.name = prefixed;
        }

        // `rule.parent.insertBefore(rule, cloned)`
        insert_before_at_path(root, path, cloned);
        Some(())
    }

    /// JS: `process(node)`.
    /// ```js
    /// process(node) {
    ///   let parent = this.parentPrefix(node)
    ///   for (let prefix of this.prefixes) {
    ///     if (!parent || parent === prefix) {
    ///       this.add(node, prefix)
    ///     }
    ///   }
    /// }
    /// ```
    pub fn process(&mut self, root: &mut postcss_core::Node, path: &[usize]) {
        let parent = parent_prefix_cached_mut(root, path);
        let prefixes = self.prefixer.prefixes.clone();

        // Track the current path of the *original* node. Each successful
        // `insert_before_at_path` shifts the original's index up by 1
        // because the clone is spliced at the original's slot. JS doesn't
        // hit this because it holds a node reference that auto-follows.
        let mut current_path = path.to_vec();

        for prefix in &prefixes {
            let allow = match &parent {
                ParentPrefix::None => true,
                ParentPrefix::Some(p) => p == prefix,
            };
            if allow {
                if self.add(root, &current_path, prefix).is_some() {
                    if let Some(last) = current_path.last_mut() {
                        *last += 1;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    #[test]
    fn add_inserts_prefixed_at_rule_clone_before() {
        // `@keyframes a { ... }` → after add("-webkit-"):
        // `@-webkit-keyframes a { ... }\n@keyframes a { ... }`.
        let mut r = parse("@keyframes a { from { color: red; } }").unwrap();
        let mut at_rule = AtRuleBase::new(
            "keyframes".into(),
            vec!["-webkit-".into()],
            0,
        );
        at_rule.add(&mut r.root, &[0], "-webkit-");
        let out = stringify(&r);
        assert!(out.contains("@-webkit-keyframes a"));
        assert!(out.contains("@keyframes a"));
    }

    #[test]
    fn add_idempotent_when_prefixed_sibling_exists() {
        let mut r = parse(
            "@-webkit-keyframes a { from { color: red; } }\n@keyframes a { from { color: red; } }",
        )
        .unwrap();
        let len_before = r.root.nodes().unwrap().len();
        let mut at_rule = AtRuleBase::new(
            "keyframes".into(),
            vec!["-webkit-".into()],
            0,
        );
        // path [1] points at `@keyframes a` — adding `-webkit-` should
        // see the existing sibling and skip.
        let result = at_rule.add(&mut r.root, &[1], "-webkit-");
        assert!(result.is_none());
        assert_eq!(r.root.nodes().unwrap().len(), len_before);
    }

    #[test]
    fn process_emits_prefixed_clones_for_each_prefix() {
        let mut r = parse("@keyframes a { from { color: red; } }").unwrap();
        let mut at_rule = AtRuleBase::new(
            "keyframes".into(),
            vec!["-webkit-".into(), "-moz-".into()],
            0,
        );
        at_rule.process(&mut r.root, &[0]);
        let out = stringify(&r);
        assert!(out.contains("@-webkit-keyframes"));
        assert!(out.contains("@-moz-keyframes"));
        assert!(out.contains("@keyframes"));
    }

    #[test]
    fn process_skips_prefix_when_parent_already_prefixed() {
        // The at-rule is itself already `@-webkit-keyframes`. Processing
        // for prefixes [-webkit-, -moz-] should only allow -webkit-
        // (parent_prefix matches), but the existing sibling check kicks
        // in for -webkit-, so net effect: only the -moz- attempt is
        // reached, but it's filtered by `parent === prefix` and skipped.
        let mut r = parse(
            "@-webkit-keyframes a { from { color: red; } }",
        )
        .unwrap();
        let mut at_rule = AtRuleBase::new(
            "keyframes".into(),
            vec!["-moz-".into()],
            0,
        );
        at_rule.process(&mut r.root, &[0]);
        let out = stringify(&r);
        // -moz- is filtered out by `parent === prefix` (parent is
        // -webkit-, prefix is -moz-) — no -moz- variant emitted.
        assert!(!out.contains("@-moz-keyframes"));
    }
}
