//! crates/postcss-nested
//! Byte-for-byte Rust port of `postcss-nested@5.0.6`.
//! See `crates/PARITY_VERSIONS.md` Anomaly #1 — version pinned to 5.x; the
//! v5 → v6 rewrite changed selector merging semantics. Do NOT consult v6.
//!
//! Upstream source: `crates/_vendor/postcss-nested-5.0.6/index.js` (215 LOC).
//! Single source file maps 1:1 to this single Rust file.
//!
//! ## Lifecycle
//!
//! Upstream is a postcss **Rule visitor** — `Rule(rule, { Rule })` is invoked
//! on every Rule encountered (depth-first, document-order), and re-visits
//! rules promoted to siblings during the walk. The Rust port simulates this
//! with a forward-iterating container walker that:
//!
//! 1. Iterates `parent.nodes` by index.
//! 2. For each Rule: invokes `visit_rule`, which removes the rule from its
//!    parent, processes the rule's children, then re-inserts the rule plus
//!    any promoted siblings at the same position.
//! 3. The cursor advances 1 each iteration. Promoted siblings end up at
//!    `i + 1`, `i + 2`, ... and are visited on subsequent iterations.
//! 4. AtRules with bodies are recursed INTO.
//!
//! No re-walk pass is needed because postcss-nested's promotion always
//! moves children OUT of the rule (not back in). After visiting a rule,
//! its remaining children (decls/comments only, no rules) need no further
//! processing.

use postcss_core::{
    container::remove_at, Node, NodeKind, PluginError, PluginResult, Root, Rule,
};
use postcss_selector_parser::nodes::{Node as SelNode, NodeKind as SelKind, Spaces as SelSpaces};
use postcss_selector_parser::{stringify as sel_stringify, Processor};

#[derive(Debug, Clone, Default)]
pub struct PostcssNestedOpts {
    /// At-rule names that bubble up (their bodies stay separate; the
    /// at-rule wraps each child rule).
    pub bubble: Vec<String>,
    /// At-rule names that unwrap (their bodies are flattened into the
    /// parent's children, the at-rule itself is removed).
    pub unwrap: Vec<String>,
    /// `opts.preserveEmpty` — when `true`, empty rules left behind after
    /// unwrapping are kept rather than removed.
    pub preserve_empty: bool,
}

const PLUGIN_NAME: &str = "postcss-nested";

// ----------------------------------------------------------------------------
// Helpers (mirror upstream module-level functions)
// ----------------------------------------------------------------------------

/// `atruleNames(defaults, custom)` — upstream lines 113-125. Combines
/// hard-coded defaults with user-supplied entries, stripping a leading
/// `@` from each custom entry. Returns a Vec for membership checks.
fn atrule_names(defaults: &[&str], custom: &[String]) -> Vec<String> {
    let mut list: Vec<String> = defaults.iter().map(|s| s.to_string()).collect();
    for entry in custom {
        let name = entry.strip_prefix('@').unwrap_or(entry).to_string();
        if !list.iter().any(|n| n == &name) {
            list.push(name);
        }
    }
    list
}

/// `parse(str, rule)` — upstream lines 3-18. Returns the first Selector
/// child of the parsed Root (`root.at(0)`).
///
/// On parse failure mirror upstream's two-branch error:
///   - If `str` contains `:`, throw `"Missed semicolon"`.
///   - Otherwise propagate the underlying parser message.
/// Both bind to `rule_ctx` for line/col info via `PluginError::from_node`.
fn parse_selector(str: &str, rule_ctx: &Node) -> Result<SelNode, PluginError> {
    match Processor::new().ast_sync(str) {
        Ok(root) => Ok(root.nodes.into_iter().next().unwrap_or_else(SelNode::selector)),
        Err(e) => {
            let msg = if str.contains(':') {
                "Missed semicolon".to_string()
            } else {
                e.message
            };
            Err(PluginError::from_node(PLUGIN_NAME, msg, rule_ctx))
        }
    }
}

/// `replace(nodes, parent)` — upstream lines 20-38. Substitutes every
/// Nesting (`&`) descendant of `nodes` with a clone of `parent` (or the
/// `&`-substituted value when `nesting.value != '&'`). Returns whether
/// any substitution occurred.
///
/// **Postcss-selector-parser quirk** (Rust port lacks descendant-combinator
/// emission — JS upstream emits an explicit `Combinator{value: " "}` for
/// whitespace, while our parser stores the whitespace as the next node's
/// `spaces.before`). To preserve byte-clean output for `.b & { ... }`-style
/// inputs, the Nesting's `spaces` are transferred onto the replacement
/// node. In JS this transfer isn't needed because the Combinator sits
/// BETWEEN the previous selector and the Nesting; here it's needed because
/// the space is fused onto the Nesting itself.
fn replace_nesting(nodes: &mut SelNode, parent: &SelNode) -> bool {
    let mut replaced = false;
    let mut i = 0;
    while i < nodes.nodes.len() {
        let is_nesting = nodes.nodes[i].kind == SelKind::Nesting;
        if is_nesting {
            let nesting_spaces = nodes.nodes[i].spaces.clone();
            let nesting_value = nodes.nodes[i].value.clone();
            let mut new_node = if nesting_value != "&" {
                let cloned_parent = parent.clone();
                let parent_str = sel_stringify(&cloned_parent);
                // Upstream `index.js:26` uses `i.value.replace('&', ...)` —
                // JS String.prototype.replace with a string pattern only
                // replaces the FIRST occurrence. Rust's `str::replace`
                // would replace ALL — use `replacen` with count=1 to
                // match JS exactly. Affects nesting values like `&-&`.
                let new_value = nesting_value.replacen('&', &parent_str, 1);
                match Processor::new().ast_sync(&new_value) {
                    Ok(root) => root
                        .nodes
                        .into_iter()
                        .next()
                        .unwrap_or_else(SelNode::selector),
                    Err(_) => SelNode::selector(),
                }
            } else {
                parent.clone()
            };
            // Transfer Nesting's spaces onto the replacement so the
            // surrounding whitespace survives stringification. See the
            // doc comment above for why this transfer is needed in the
            // Rust port but not in JS.
            new_node.spaces = nesting_spaces;
            new_node.raw_value = None;
            nodes.nodes[i] = new_node;
            replaced = true;
            i += 1;
        } else if !nodes.nodes[i].nodes.is_empty() {
            if replace_nesting(&mut nodes.nodes[i], parent) {
                replaced = true;
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    replaced
}

/// `selectors(parent, child)` — upstream lines 40-58. Computes the
/// merged selector list for `child` nested inside `parent`.
fn selectors_of(
    parent_rule: &Node,
    parent_selector_str: &str,
    child_rule: &Node,
    child_selector_str: &str,
) -> Result<Vec<String>, PluginError> {
    let mut result: Vec<String> = Vec::new();
    let parent_selectors = postcss_core::list::comma(parent_selector_str);
    let child_selectors = postcss_core::list::comma(child_selector_str);

    for parent_sel in &parent_selectors {
        let parent_node = parse_selector(parent_sel, parent_rule)?;
        for child_sel in &child_selectors {
            // upstream `if (j.length)` — skip empty entries.
            if child_sel.is_empty() {
                continue;
            }
            let mut node = parse_selector(child_sel, child_rule)?;
            let replaced = replace_nesting(&mut node, &parent_node);
            if !replaced {
                // upstream: prepend combinator(' ') THEN prepend parent.clone().
                // Two prepends-at-front yield order:
                //   [parent.clone(), combinator(' '), ...original children]
                let combinator = SelNode {
                    kind: SelKind::Combinator,
                    value: " ".to_string(),
                    spaces: SelSpaces::default(),
                    nodes: Vec::new(),
                    raw_value: None,
                    attribute: None,
                    attribute_spaces: None,
                    source_index: None,
                };
                node.nodes.insert(0, combinator);
                node.nodes.insert(0, parent_node.clone());
            }
            result.push(sel_stringify(&node));
        }
    }
    Ok(result)
}

/// `pickDeclarations(selector, declarations, after, Rule)` — upstream
/// lines 99-111. Builds a fresh wrapper Rule with `selector`, appends
/// `declarations`. The caller is responsible for splicing the wrapper
/// into `parent` after the appropriate sample.
///
/// The wrapper carries no explicit `raws` — `raws.between`,
/// `raws.semicolon`, and `raws.after` are all left `None`. The
/// stringifier in `postcss-core` derives them via the `rawBeforeOpen`,
/// `rawSemicolon`, and `rawBeforeClose` scanners (mirroring upstream
/// `postcss/lib/stringifier.js::raw`), which produces the same bytes
/// JS upstream emits for `new Rule({ selector, nodes: [] })`.
fn build_wrapper_rule(selector: &str, declarations: Vec<Node>) -> Node {
    let mut wrapper = Node::new(NodeKind::Rule(Rule {
        selector: selector.to_string(),
        nodes: Vec::new(),
    }));
    // Mirror postcss base Container.normalize: when sample is defined and
    // child.raws.before is undefined, copy stripped sample.raws.before.
    // The first append has sample = undefined (this.last on empty container),
    // so child.raws.before is left unchanged.
    for (idx, mut decl) in declarations.into_iter().enumerate() {
        if idx > 0 {
            let sample_before = wrapper
                .nodes()
                .and_then(|n| n.last())
                .and_then(|s| s.raws.before.clone());
            if decl.raws.before.is_none() {
                if let Some(sb) = sample_before {
                    decl.raws.before = Some(strip_non_whitespace(&sb));
                }
            }
        }
        wrapper.nodes_mut().unwrap().push(decl);
    }
    wrapper
}

/// `createFnAtruleChilds(bubble)` — upstream lines 69-97. Given a parent
/// `rule` (referenced by `rule_ctx` + `rule_selector_str`), iterates
/// `atrule.nodes`:
///   - decls/comments → moved into `children` collector.
///   - rules (when bubbling) → selectors rewritten via `selectors_of`.
///   - at-rules with body and bubble-listed name → recurse with bubbling=true.
///   - other at-rules → moved into `children` collector.
/// If bubbling AND `children` non-empty, clone `rule` with empty nodes,
/// append children, prepend clone to atrule.
fn atrule_childs(
    rule_ctx: &Node,
    rule_selector_str: &str,
    atrule: &mut Node,
    bubbling: bool,
    bubble: &[String],
) -> Result<(), PluginError> {
    let mut children: Vec<Node> = Vec::new();

    let mut i = 0;
    loop {
        let len = atrule.nodes().map(|n| n.len()).unwrap_or(0);
        if i >= len {
            break;
        }
        let kind_tag = match &atrule.nodes().unwrap()[i].kind {
            NodeKind::Comment(_) => 0,
            NodeKind::Declaration(_) => 1,
            NodeKind::Rule(_) => 2,
            NodeKind::AtRule(_) => 3,
            _ => 4,
        };
        match kind_tag {
            0 | 1 => {
                // comment / decl. Upstream JS `children.push(child)` stores
                // a reference WITHOUT removing from atrule.nodes — the
                // physical move happens later via `clone.append(child)`
                // which postcss handles by removing from the old parent.
                // For bubbling=true we mirror by removing here (so we can
                // collect into `children` and later push into the clone).
                // For bubbling=false the children list is unused and JS
                // never moves these — leave them in place.
                if bubbling {
                    let n = atrule.nodes_mut().unwrap().remove(i);
                    children.push(n);
                } else {
                    i += 1;
                }
            }
            2 => {
                if bubbling {
                    let child_sel = match &atrule.nodes().unwrap()[i].kind {
                        NodeKind::Rule(r) => r.selector.clone(),
                        _ => unreachable!(),
                    };
                    let new_selectors = selectors_of(
                        rule_ctx,
                        rule_selector_str,
                        &atrule.nodes().unwrap()[i],
                        &child_sel,
                    )?;
                    if let NodeKind::Rule(r) = &mut atrule.nodes_mut().unwrap()[i].kind {
                        r.set_selectors(&new_selectors);
                    }
                    atrule.nodes_mut().unwrap()[i].raws.selector = None;
                }
                i += 1;
            }
            3 => {
                let (has_block, name) = match &atrule.nodes().unwrap()[i].kind {
                    NodeKind::AtRule(a) => (a.has_block, a.name.clone()),
                    _ => unreachable!(),
                };
                if has_block && bubble.iter().any(|b| b == &name) {
                    // Recurse with bubbling=true regardless of our own state.
                    let nested = &mut atrule.nodes_mut().unwrap()[i];
                    atrule_childs(rule_ctx, rule_selector_str, nested, true, bubble)?;
                    i += 1;
                } else if bubbling {
                    let n = atrule.nodes_mut().unwrap().remove(i);
                    children.push(n);
                } else {
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    if bubbling && !children.is_empty() {
        let mut clone = clone_rule_with_empty_nodes(rule_ctx, rule_selector_str);
        // Append each child mirroring postcss base normalize semantics
        // (sample = clone.last, copy stripped before when undefined).
        for (idx, mut child) in children.into_iter().enumerate() {
            if idx > 0 {
                let sample_before = clone
                    .nodes()
                    .and_then(|n| n.last())
                    .and_then(|s| s.raws.before.clone());
                if child.raws.before.is_none() {
                    if let Some(sb) = sample_before {
                        child.raws.before = Some(strip_non_whitespace(&sb));
                    }
                }
            }
            clone.nodes_mut().unwrap().push(child);
        }

        // upstream: atrule.prepend(clone). For non-Root parent base
        // Container.normalize fires with sample = atrule.first.
        if let Some(nodes) = atrule.nodes_mut() {
            if clone.raws.before.is_none() {
                if let Some(sb) = nodes.first().and_then(|n| n.raws.before.clone()) {
                    clone.raws.before = Some(strip_non_whitespace(&sb));
                }
            }
            nodes.insert(0, clone);
        }
    }

    Ok(())
}

/// Clone a Rule node with `nodes: []` and the supplied selector. Mirrors
/// `rule.clone({ nodes: [] })` upstream.
///
/// Upstream `cloneNode` deep-copies every own property including `raws`,
/// then the `{ nodes: [] }` override replaces nodes with an empty array.
/// `raws.selector` is preserved verbatim — when the selector value
/// matches `raws.selector.value`, the postcss stringifier emits the
/// raw form, preserving byte-fidelity. Do NOT clear raws.selector.
fn clone_rule_with_empty_nodes(rule_node: &Node, selector: &str) -> Node {
    let mut clone = rule_node.clone();
    if let NodeKind::Rule(r) = &mut clone.kind {
        r.nodes.clear();
        r.selector = selector.to_string();
    }
    clone
}

/// Insert `add` immediately AFTER `parent.nodes[exist_index]` mirroring
/// `node.after(add)` → `parent.insertAfter(node, add)`:
///
/// - Non-Root parent: base `Container.normalize(add, sample)` — when
///   `add.raws.before` is `None` and `sample.raws.before` is defined,
///   copy with non-whitespace stripped.
/// - Root parent: `Root.normalize` calls `super.normalize(child)` WITHOUT
///   sample (so base doesn't fire), then if `first !== sample`
///   (i.e. `exist_index > 0`) sets `add.raws.before = sample.raws.before`
///   verbatim.
fn insert_after_with_normalize(
    parent: &mut Node,
    exist_index: usize,
    mut add: Node,
    parent_is_root: bool,
) {
    if !parent_is_root {
        if add.raws.before.is_none() {
            if let Some(nodes) = parent.nodes() {
                if let Some(sample) = nodes.get(exist_index) {
                    if let Some(sb) = &sample.raws.before {
                        add.raws.before = Some(strip_non_whitespace(sb));
                    }
                }
            }
        }
    } else if exist_index > 0 {
        if let Some(nodes) = parent.nodes() {
            if let Some(sample) = nodes.get(exist_index) {
                add.raws.before = sample.raws.before.clone();
            }
        }
    }

    if let Some(nodes) = parent.nodes_mut() {
        let at = (exist_index + 1).min(nodes.len());
        nodes.insert(at, add);
    }
}

/// JS `value.replace(/\S/g, '')` — strip every non-whitespace char.
fn strip_non_whitespace(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_whitespace() || *c == '\u{FEFF}')
        .collect()
}

/// Whether a node is a comment.
fn is_comment(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Comment(_))
}

// ----------------------------------------------------------------------------
// The Rule visitor — upstream lines 144-211 (`Rule (rule, { Rule }) { ... }`)
// ----------------------------------------------------------------------------

fn visit_rule(
    parent: &mut Node,
    rule_index: usize,
    bubble: &[String],
    unwrap: &[String],
    preserve_empty: bool,
) -> Result<bool, PluginError> {
    let parent_is_root = matches!(parent.kind, NodeKind::Root(_));

    // Take rule out of parent for unrestricted mutation.
    let mut rule = parent.nodes_mut().unwrap().remove(rule_index);
    let rule_selector_str = match &rule.kind {
        NodeKind::Rule(r) => r.selector.clone(),
        _ => unreachable!("visit_rule called with non-Rule node"),
    };
    // `rule_ctx` for error messages — use a snapshot clone.
    let rule_ctx = rule.clone();

    // Promoted siblings to insert after `rule` (in order). When `rule` is
    // ultimately removed (unwrapped + empty), these still get inserted at
    // `rule_index..` and rule itself is dropped.
    let mut promoted: Vec<Node> = Vec::new();

    let mut unwrapped = false;
    let mut copy_declarations = false;
    let mut declarations: Vec<Node> = Vec::new();

    let mut i = 0usize;
    loop {
        let len = rule.nodes().map(|n| n.len()).unwrap_or(0);
        if i >= len {
            break;
        }
        let kind_tag = match &rule.nodes().unwrap()[i].kind {
            NodeKind::Rule(_) => 0,
            NodeKind::AtRule(_) => 1,
            NodeKind::Declaration(_) => 2,
            NodeKind::Comment(_) => 3,
            _ => 4,
        };

        match kind_tag {
            0 => {
                // Child Rule branch.
                if !declarations.is_empty() {
                    let decls = std::mem::take(&mut declarations);
                    let wrapper = build_wrapper_rule(&rule_selector_str, decls);
                    promoted.push(wrapper);
                }
                copy_declarations = true;
                unwrapped = true;

                // Compute new selectors with parent context.
                let child_sel_str = match &rule.nodes().unwrap()[i].kind {
                    NodeKind::Rule(r) => r.selector.clone(),
                    _ => unreachable!(),
                };
                let new_selectors = selectors_of(
                    &rule_ctx,
                    &rule_selector_str,
                    &rule.nodes().unwrap()[i],
                    &child_sel_str,
                )?;
                let child_mut = &mut rule.nodes_mut().unwrap()[i];
                if let NodeKind::Rule(c) = &mut child_mut.kind {
                    c.set_selectors(&new_selectors);
                }
                child_mut.raws.selector = None;

                // pickComment(child.prev(), after).
                let mut effective_i = i;
                if effective_i > 0 && is_comment(&rule.nodes().unwrap()[effective_i - 1]) {
                    let comment = rule.nodes_mut().unwrap().remove(effective_i - 1);
                    promoted.push(comment);
                    effective_i -= 1;
                }

                // after.after(child) — move child to promoted.
                let child = rule.nodes_mut().unwrap().remove(effective_i);
                promoted.push(child);
                i = effective_i; // don't advance; sibling now sits at i
            }
            1 => {
                // AtRule branch.
                let (at_name, has_block, params) = match &rule.nodes().unwrap()[i].kind {
                    NodeKind::AtRule(a) => (a.name.clone(), a.has_block, a.params.clone()),
                    _ => unreachable!(),
                };
                // Upstream dumps pending declarations at the top of EVERY
                // at-rule sub-branch (including copy_declarations
                // fall-through). We replicate that here.
                if !declarations.is_empty() {
                    let decls = std::mem::take(&mut declarations);
                    let wrapper = build_wrapper_rule(&rule_selector_str, decls);
                    promoted.push(wrapper);
                }

                if at_name == "at-root" {
                    unwrapped = true;
                    // Take atrule out for mutation.
                    let mut atrule = rule.nodes_mut().unwrap().remove(i);
                    atrule_childs(&rule_ctx, &rule_selector_str, &mut atrule, false, bubble)?;
                    // child.nodes
                    let mut nodes_out: Vec<Node> = if let NodeKind::AtRule(a) = &mut atrule.kind {
                        std::mem::take(&mut a.nodes)
                    } else {
                        Vec::new()
                    };
                    if !params.is_empty() {
                        // Fresh Rule with no raws — the postcss-core
                        // stringifier's rawBeforeOpen / rawSemicolon /
                        // rawBeforeClose scanners derive the byte-shape
                        // defaults from the surrounding tree.
                        let wrapper = Node::new(NodeKind::Rule(Rule {
                            selector: params.clone(),
                            nodes: nodes_out,
                        }));
                        nodes_out = vec![wrapper];
                    }
                    for n in nodes_out {
                        promoted.push(n);
                    }
                    // atrule itself is discarded (child.remove() in JS).
                    // i stays the same.
                } else if has_block && bubble.iter().any(|b| b == &at_name) {
                    copy_declarations = true;
                    unwrapped = true;
                    let mut atrule = rule.nodes_mut().unwrap().remove(i);
                    atrule_childs(&rule_ctx, &rule_selector_str, &mut atrule, true, bubble)?;

                    // pickComment then push.
                    if i > 0 && is_comment(&rule.nodes().unwrap()[i - 1]) {
                        let comment = rule.nodes_mut().unwrap().remove(i - 1);
                        promoted.push(comment);
                        i -= 1;
                    }
                    promoted.push(atrule);
                } else if has_block && unwrap.iter().any(|u| u == &at_name) {
                    copy_declarations = true;
                    unwrapped = true;
                    let mut atrule = rule.nodes_mut().unwrap().remove(i);
                    atrule_childs(&rule_ctx, &rule_selector_str, &mut atrule, false, bubble)?;

                    if i > 0 && is_comment(&rule.nodes().unwrap()[i - 1]) {
                        let comment = rule.nodes_mut().unwrap().remove(i - 1);
                        promoted.push(comment);
                        i -= 1;
                    }
                    promoted.push(atrule);
                } else if copy_declarations {
                    // Treat as a declaration — push to declarations list.
                    let n = rule.nodes_mut().unwrap().remove(i);
                    declarations.push(n);
                } else {
                    i += 1;
                }
            }
            2 => {
                if copy_declarations {
                    let decl = rule.nodes_mut().unwrap().remove(i);
                    declarations.push(decl);
                } else {
                    i += 1;
                }
            }
            3 => {
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    // Trailing declarations dump.
    if !declarations.is_empty() {
        let decls = std::mem::take(&mut declarations);
        let wrapper = build_wrapper_rule(&rule_selector_str, decls);
        promoted.push(wrapper);
    }

    // Removal check.
    let mut should_remove = false;
    if unwrapped && !preserve_empty {
        rule.raws.semicolon = Some(true);
        if rule.nodes().map(|n| n.is_empty()).unwrap_or(true) {
            should_remove = true;
        }
    }

    // Re-insert rule at rule_index, then chain-insert promoted siblings.
    parent.nodes_mut().unwrap().insert(rule_index, rule);
    let mut after_index = rule_index;
    for p in promoted {
        insert_after_with_normalize(parent, after_index, p, parent_is_root);
        after_index += 1;
    }

    if should_remove {
        // remove_at fires Root.removeChild override (raws-transfer when
        // removing first child of root with at least one sibling left).
        remove_at(parent, rule_index);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Walk every container in the tree, applying the Rule visitor.
fn walk_container(
    container: &mut Node,
    bubble: &[String],
    unwrap: &[String],
    preserve_empty: bool,
) -> Result<(), PluginError> {
    let mut i = 0usize;
    loop {
        let len = container.nodes().map(|n| n.len()).unwrap_or(0);
        if i >= len {
            break;
        }
        let kind_tag = match &container.nodes().unwrap()[i].kind {
            NodeKind::Rule(_) => 0,
            NodeKind::AtRule(a) if a.has_block => 1,
            _ => 2,
        };
        match kind_tag {
            0 => {
                let removed = visit_rule(container, i, bubble, unwrap, preserve_empty)?;
                if !removed {
                    i += 1;
                }
                // If removed, the next iteration visits whatever now sits
                // at `i` (one of the promoted siblings).
            }
            1 => {
                let inner = &mut container.nodes_mut().unwrap()[i];
                walk_container(inner, bubble, unwrap, preserve_empty)?;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    Ok(())
}

pub fn postcss_nested(root: &mut Root, opts: &PostcssNestedOpts) -> PluginResult {
    let bubble = atrule_names(&["media", "supports"], &opts.bubble);
    let unwrap = atrule_names(
        &[
            "document",
            "font-face",
            "keyframes",
            "-webkit-keyframes",
            "-moz-keyframes",
        ],
        &opts.unwrap,
    );
    walk_container(&mut root.root, &bubble, &unwrap, opts.preserve_empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        postcss_nested(&mut root, &PostcssNestedOpts::default()).unwrap();
        stringify(&root)
    }

    #[test]
    fn passes_through_no_nesting() {
        let css = "a { color: red; }";
        assert_eq!(run(css), css);
    }

    #[test]
    fn unwraps_nested_rule() {
        let out = run("a { color: red; b { color: blue } }");
        assert!(out.contains("a b"), "got: {out}");
        assert!(out.contains("color: red"), "got: {out}");
        assert!(out.contains("color: blue"), "got: {out}");
    }

    #[test]
    fn ampersand_substitution() {
        let out = run("a { &.b { color: red } }");
        assert!(out.contains("a.b"), "got: {out}");
    }

    #[test]
    fn bubble_at_media() {
        let out = run("a { @media (max-width: 100px) { color: red } }");
        assert!(out.contains("@media"), "got: {out}");
        assert!(out.contains("color: red"), "got: {out}");
    }

    /// Regression: `replace_nesting` uses `replacen('&', ..., 1)` to match
    /// JS `String.prototype.replace('&', ...)` first-only semantics. The
    /// in-tree selector parser always produces `Nesting.value == "&"`, so
    /// this is defensive — it only diverges when a consumer hand-mutates
    /// `i.value` to contain multiple `&`. Direct unit test on the helper.
    #[test]
    fn replace_nesting_first_only_when_value_has_two_ampersands() {
        let mut nesting = SelNode::selector();
        nesting.nodes.push(SelNode {
            kind: SelKind::Nesting,
            value: "&-&".to_string(),
            spaces: SelSpaces::default(),
            nodes: Vec::new(),
            raw_value: None,
            attribute: None,
            attribute_spaces: None,
            source_index: None,
        });
        let parent = Processor::new()
            .ast_sync(".foo")
            .unwrap()
            .nodes
            .into_iter()
            .next()
            .unwrap();
        let replaced = replace_nesting(&mut nesting, &parent);
        assert!(replaced);
        // After the call, the Nesting at index 0 was replaced with a
        // parsed Selector whose raw stringified form contains `.foo-&`
        // — first `&` substituted, second `&` left literal. If the bug
        // were present the output would contain `.foo-.foo`.
        let out = sel_stringify(&nesting);
        assert!(
            out.contains(".foo-&"),
            "first-only replacement violated, got: {out}"
        );
        assert!(
            !out.contains(".foo-.foo"),
            "second & was wrongly replaced, got: {out}"
        );
    }

    /// Regression: `clone_rule_with_empty_nodes` must NOT clear
    /// `raws.selector`. Upstream `clone({ nodes: [] })` preserves all
    /// raws verbatim. When raws.selector.value matches the cloned
    /// rule's selector, the raw form is emitted byte-for-byte.
    #[test]
    fn bubble_clone_preserves_raw_selector_form() {
        // Selector raws capture the trailing comment via raw_record;
        // the cloned wrapper inside @media must re-emit it. The outer
        // `a/*x*/` rule is removed (empty after bubble extraction), so
        // the only remaining occurrence is in the cloned wrapper.
        let css = "a/*x*/ { @media (max-width: 1px) { color: red } }";
        let out = run(css);
        assert!(out.contains("@media"), "got: {out}");
        assert!(
            out.contains("a/*x*/"),
            "raws.selector not preserved on clone, got: {out}"
        );
    }
}
