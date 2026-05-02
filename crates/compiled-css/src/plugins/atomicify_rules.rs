//! Port of `packages/css/src/plugins/atomicify-rules.ts`.
//!
//! **CRITICAL** plugin per `crates/EXECUTION_PLAN.md` 4d. This is the one
//! whose hash output becomes class names. Output bytes MUST be
//! bit-identical to the JS implementation for every input.
//!
//! ## "Bugs are features" — the JS quirks we replicate
//!
//! 1. **`undefined` → `"undefined"` in the group-hash input.** The hash
//!    template is `` `${prefix}${atRule}${selectors}${prop}` ``. When
//!    `opts.atRule` is undefined (top-level call), JS's template literal
//!    coerces it to the literal 7-char string `"undefined"`. We bake that
//!    string in as the default for [`AtomicifyInternalOpts::at_rule`].
//!
//! 2. **`node.important` (boolean) coerced to `"true"` in the value-hash
//!    input.** Upstream does `node.value + node.important` when important.
//!    JS coerces the boolean to `"true"`. The class-name hash for
//!    `color: red !important` is `hash("redtrue")` not `hash("red!important")`.
//!
//! 3. **`hash(input).slice(0, 4)`** — JS slice doesn't pad. If a hash
//!    happens to be shorter than 4 chars (only `"0"` for empty input
//!    today), the resulting class name is also short. We use
//!    `chars().take(4).collect()` which has the same no-pad behavior.
//!
//! 4. **Compression-map key omits the leading `_`.** Upstream:
//!    `classNameCompressionMap[fullClassName.slice(1)]`. The full class
//!    is `_<group><value>`; the lookup key is `<group><value>` (no
//!    underscore). The compressed value is then used in
//!    `replaceNestingSelector` BUT the callback still receives the
//!    full (uncompressed) class name.
//!
//! 5. **`selector.replace(/&/g, ...)`** — every occurrence of `&` is
//!    replaced. `&&` becomes the doubled class (e.g. `._foo._foo`).

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;

use postcss_core::container::{each_mut, Mutation};
use postcss_core::node::{RawValue, Raws, Source};
use postcss_core::{
    AtRule, Declaration, Node, NodeKind, PluginError, PluginResult, Root, Rule,
};
use sjcompiled_utils::hash;

#[derive(Debug, Clone, Default)]
pub struct AtomicifyRulesOpts {
    /// Maps long class hashes (without the leading `_`) to short
    /// identifiers. Iteration order matters; upstream uses Object
    /// insertion order, which we preserve via `IndexMap`.
    pub class_name_compression_map: Option<IndexMap<String, String>>,
    pub class_hash_prefix: Option<String>,
    /// Class names produced by the run are pushed here in document
    /// order — equivalent to upstream's `callback(className)`.
    pub class_names: Vec<String>,
}

/// Internal opts threaded through recursive calls. Mirrors upstream
/// `PluginOpts`. Public-facing fields stay on `AtomicifyRulesOpts`;
/// recursion-only state (`at_rule`, `selectors`) lives here.
struct AtomicifyInternalOpts<'a> {
    public: &'a mut AtomicifyRulesOpts,
    /// Concatenation of every enclosing at-rule's `name + params`. None
    /// at the top level — JS template-literal coerces this to the
    /// literal string `"undefined"` for the hash input.
    at_rule: Option<String>,
    /// Selectors of the current Rule (one per comma-split group). Empty
    /// at the top level (`['']` is what `buildAtomicSelector` uses to
    /// mean "no enclosing rule").
    selectors: Vec<String>,
}

static CSS_IDENTIFIER_RE: Lazy<Regex> = Lazy::new(|| {
    // Mirrors `^[a-zA-Z\-_]+[a-zA-Z\-_0-9]*$` upstream.
    Regex::new(r"^[a-zA-Z\-_]+[a-zA-Z\-_0-9]*$").expect("identifier regex")
});

/// Public plugin entrypoint.
pub fn atomicify_rules(root: &mut Root, opts: &mut AtomicifyRulesOpts) -> PluginResult {
    if let Some(prefix) = opts.class_hash_prefix.as_deref() {
        if !is_css_identifier_valid(prefix) {
            return Err(PluginError::generic(
                "atomicify-rules",
                format!(
                    "{prefix} isn't a valid CSS identifier. Accepted characters are ^[a-zA-Z\\-_]+[a-zA-Z\\-_0-9]*$"
                ),
            ));
        }
    }

    // Top-level walk. We can't return Result from each_mut's closure,
    // so capture errors via a local Option and stop iterating once set.
    let mut error: Option<PluginError> = None;
    each_mut(&mut root.root, |node, _ctx| {
        if error.is_some() {
            return Mutation::Keep;
        }

        let mut internal = AtomicifyInternalOpts {
            public: opts,
            at_rule: None,
            selectors: Vec::new(),
        };

        match &node.kind {
            NodeKind::AtRule(_) => match can_atomicify_atrule(node) {
                Ok(true) => match atomicify_atrule(node, &mut internal) {
                    Ok(new_node) => Mutation::Replace(new_node),
                    Err(e) => { error = Some(e); Mutation::Keep }
                },
                Ok(false) => Mutation::Keep,
                Err(e) => { error = Some(e); Mutation::Keep }
            },
            NodeKind::Rule(_) => match atomicify_rule(node, &mut internal) {
                Ok(new_rules) => Mutation::ReplaceMany(new_rules),
                Err(e) => { error = Some(e); Mutation::Keep }
            },
            NodeKind::Declaration(_) => {
                let new_rule = atomicify_decl(node, &mut internal);
                Mutation::Replace(new_rule)
            }
            NodeKind::Comment(_) => Mutation::Remove,
            _ => Mutation::Keep,
        }
    });

    match error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

// --------------------------------------------------------------------------
// Atomic class-name generation
// --------------------------------------------------------------------------

/// Mirrors `isCssIdentifierValid` upstream.
fn is_css_identifier_valid(value: &str) -> bool {
    CSS_IDENTIFIER_RE.is_match(value)
}

/// Mirrors `atomicClassName` upstream.
///
/// `selectors_for_hash` is the single-element list that
/// `buildAtomicSelector` passes via `{ ...opts, selectors: [normalizedSelector] }`.
fn atomic_class_name(
    decl: &Declaration,
    selectors_for_hash: &[String],
    internal: &AtomicifyInternalOpts,
) -> String {
    let prefix = internal
        .public
        .class_hash_prefix
        .as_deref()
        .unwrap_or("");
    // Upstream `${opts.atRule}` with undefined coerces to "undefined".
    // We bake that quirk in here as the literal default.
    let at_rule = internal.at_rule.as_deref().unwrap_or("undefined");
    let selectors_joined = selectors_for_hash.concat();

    let group_input = format!("{prefix}{at_rule}{selectors_joined}{}", decl.prop);
    let group: String = hash(&group_input).chars().take(4).collect();

    // `node.important` is a boolean in upstream postcss; JS coerces to
    // "true" via `${node.value}${node.important}` concatenation.
    let value_input = if decl.important {
        format!("{}true", decl.value)
    } else {
        decl.value.clone()
    };
    let value_hash: String = hash(&value_input).chars().take(4).collect();

    format!("_{group}{value_hash}")
}

/// Mirrors `normalizeSelector`. Returns `&` for empty/None input,
/// otherwise the trimmed selector with `&` prepended if `&` was absent.
fn normalize_selector(selector: Option<&str>) -> String {
    let Some(s) = selector else {
        return "&".to_string();
    };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        // Upstream `if (!selector)` covers `""` (falsy) too.
        return "&".to_string();
    }
    if !trimmed.contains('&') {
        return format!("& {trimmed}");
    }
    trimmed.to_string()
}

/// Mirrors `replaceNestingSelector` — replace EVERY `&` with
/// `.<parent_class_name>`. `String::replace` already replaces all,
/// matching JS `selector.replace(/&/g, ...)`.
fn replace_nesting_selector(selector: &str, parent_class_name: &str) -> String {
    selector.replace('&', &format!(".{parent_class_name}"))
}

/// Mirrors `buildAtomicSelector`. Side effect: pushes each generated
/// full (uncompressed) class name to `internal.public.class_names` so
/// the caller sees them in the same order as upstream's `callback`.
fn build_atomic_selector(decl: &Declaration, internal: &mut AtomicifyInternalOpts) -> String {
    // Upstream `(opts.selectors || [''])`. An empty Vec maps to the
    // top-level path (no enclosing rule).
    let inputs: Vec<String> = if internal.selectors.is_empty() {
        vec![String::new()]
    } else {
        internal.selectors.clone()
    };

    let mut out_selectors: Vec<String> = Vec::with_capacity(inputs.len());

    for input in inputs {
        let normalized = normalize_selector(if input.is_empty() { None } else { Some(&input) });
        let full_class = atomic_class_name(decl, &[normalized.clone()], internal);
        // `compressedClassName = classNameCompressionMap?.[fullClassName.slice(1)]`
        // — the lookup key omits the leading `_`.
        let compressed = internal
            .public
            .class_name_compression_map
            .as_ref()
            .and_then(|m| m.get(&full_class[1..]).cloned());
        let selector_class = compressed.unwrap_or_else(|| full_class.clone());

        out_selectors.push(replace_nesting_selector(&normalized, &selector_class));

        // Callback equivalent — push the FULL (uncompressed) class.
        internal.public.class_names.push(full_class);
    }

    out_selectors.join(", ")
}

// --------------------------------------------------------------------------
// Per-kind atomicify functions
// --------------------------------------------------------------------------

/// Mirrors `atomicifyDecl`. Builds a Rule containing one Declaration.
fn atomicify_decl(node: &Node, internal: &mut AtomicifyInternalOpts) -> Node {
    let decl = match &node.kind {
        NodeKind::Declaration(d) => d,
        _ => unreachable!("atomicify_decl called on non-Decl"),
    };
    let selector = build_atomic_selector(decl, internal);

    // newDecl: clone Declaration with raws { before:"", value:RawValue("",""), between:"" }.
    let new_decl = Node {
        kind: NodeKind::Declaration(Declaration {
            prop: decl.prop.clone(),
            value: decl.value.clone(),
            important: decl.important,
            variable: decl.variable,
        }),
        raws: Raws {
            before: Some(String::new()),
            between: Some(String::new()),
            value: Some(RawValue { value: String::new(), raw: String::new() }),
            ..Raws::default()
        },
        // Preserve source so any error tied to this decl still has the
        // original line/col info — matches upstream's `node.clone()`.
        source: node.source.clone(),
        ..Node::default()
    };

    // newRule: build a Rule with raws { before:"", after:"", between:"", selector:RawValue("","") }
    // and the assembled `selector` string.
    Node {
        kind: NodeKind::Rule(Rule {
            selector,
            nodes: vec![new_decl],
        }),
        raws: Raws {
            before: Some(String::new()),
            after: Some(String::new()),
            between: Some(String::new()),
            selector: Some(RawValue { value: String::new(), raw: String::new() }),
            ..Raws::default()
        },
        source: Source::default(),
        ..Node::default()
    }
}

/// Mirrors `atomicifyRule`. Errors on nested rules; ignores non-decls;
/// atomicifies decls. Returns the list of atomic rules (one per child
/// decl).
fn atomicify_rule(
    node: &Node,
    internal: &mut AtomicifyInternalOpts,
) -> Result<Vec<Node>, PluginError> {
    let rule = match &node.kind {
        NodeKind::Rule(r) => r,
        _ => unreachable!("atomicify_rule called on non-Rule"),
    };
    let children: &Vec<Node> = match node.nodes() {
        Some(n) => n,
        None => return Ok(Vec::new()),
    };

    // Save & restore selectors so the caller's recursion isn't disturbed.
    let saved_selectors = std::mem::take(&mut internal.selectors);
    internal.selectors = rule.get_selectors();

    let mut out: Vec<Node> = Vec::new();
    for child in children {
        match &child.kind {
            NodeKind::Rule(_) => {
                internal.selectors = saved_selectors;
                return Err(PluginError::from_node(
                    "atomicify-rules",
                    "Nested rules need to be flattened first - run the \"postcss-nested\" plugin before this.",
                    child,
                ));
            }
            NodeKind::Declaration(_) => {
                out.push(atomicify_decl(child, internal));
            }
            _ => {
                // Comments / others dropped (matches upstream
                // `if (childNode.type !== 'decl') return undefined`).
            }
        }
    }

    internal.selectors = saved_selectors;
    Ok(out)
}

/// Mirrors `canAtomicifyAtRule`. Returns `Ok(true)` for atomicifiable
/// at-rules, `Ok(false)` for ignored, `Err(...)` for forbidden or
/// unknown.
fn can_atomicify_atrule(node: &Node) -> Result<bool, PluginError> {
    let at = match &node.kind {
        NodeKind::AtRule(a) => a,
        _ => unreachable!(),
    };
    let name = at.name.as_str();

    const CAN_BE_ATOMICIFIED: &[&str] = &[
        "container",
        "-moz-document",
        "else",
        "layer",
        "media",
        "starting-style",
        "supports",
        "when",
    ];
    const FORBIDDEN: &[&str] = &["charset", "import", "namespace"];
    const IGNORED: &[&str] = &[
        "color-profile",
        "counter-style",
        "font-face",
        "font-palette-values",
        "keyframes",
        "page",
        "property",
    ];

    if CAN_BE_ATOMICIFIED.contains(&name) {
        return Ok(true);
    }
    if FORBIDDEN.contains(&name) {
        return Err(PluginError::from_node(
            "atomicify-rules",
            format!("At-rule '@{name}' cannot be used in CSS rules."),
            node,
        ));
    }
    if !IGNORED.contains(&name) {
        return Err(PluginError::from_node(
            "atomicify-rules",
            format!("Unknown at-rule '@{name}'."),
            node,
        ));
    }
    Ok(false)
}

/// Mirrors `atomicifyAtRule`. Recursively atomicifies an at-rule body.
fn atomicify_atrule(
    node: &Node,
    internal: &mut AtomicifyInternalOpts,
) -> Result<Node, PluginError> {
    let at = match &node.kind {
        NodeKind::AtRule(a) => a,
        _ => unreachable!(),
    };

    // atRuleLabel = (prev || "") + name + params  — note: NO leading
    // space between name and params, matches upstream.
    let prev_label = internal.at_rule.clone().unwrap_or_default();
    let new_label = format!("{prev_label}{}{}", at.name, at.params);
    let saved_at_rule = internal.at_rule.replace(new_label);

    let mut new_children: Vec<Node> = Vec::new();
    let empty: Vec<Node> = Vec::new();
    let body: &Vec<Node> = node.nodes().unwrap_or(&empty);
    for child in body {
        match &child.kind {
            NodeKind::AtRule(_) => match can_atomicify_atrule(child) {
                Ok(true) => {
                    let inner = atomicify_atrule(child, internal)?;
                    new_children.push(inner);
                }
                Ok(false) => {
                    new_children.push(child.clone());
                }
                Err(e) => {
                    internal.at_rule = saved_at_rule;
                    return Err(e);
                }
            },
            NodeKind::Rule(_) => {
                let rules = match atomicify_rule(child, internal) {
                    Ok(r) => r,
                    Err(e) => {
                        internal.at_rule = saved_at_rule;
                        return Err(e);
                    }
                };
                for r in rules {
                    new_children.push(r);
                }
            }
            NodeKind::Declaration(_) => {
                new_children.push(atomicify_decl(child, internal));
            }
            _ => {
                // Comments / others dropped — upstream's default branch
                // is empty.
            }
        }
    }

    internal.at_rule = saved_at_rule;

    // Build the new at-rule. Carry over name, params, has_block from
    // the original; replace raws with the upstream override.
    let new_at = AtRule {
        name: at.name.clone(),
        params: at.params.clone(),
        has_block: at.has_block,
        nodes: new_children,
    };
    Ok(Node {
        kind: NodeKind::AtRule(new_at),
        raws: Raws {
            before: Some(String::new()),
            between: Some(String::new()),
            semicolon: Some(false),
            params: Some(RawValue { value: String::new(), raw: String::new() }),
            ..Raws::default()
        },
        source: node.source.clone(),
        ..Node::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn run(css: &str) -> String {
        let mut root = parse(css).unwrap();
        let mut opts = AtomicifyRulesOpts::default();
        atomicify_rules(&mut root, &mut opts).unwrap();
        stringify(&root)
    }

    #[test]
    fn single_top_level_decl() {
        let out = run("color: blue;");
        // Group hash for `"undefined&color"` → `syaz`. Value hash for
        // `"blue"` → `13q2`.
        assert!(out.contains("._syaz13q2"), "got: {out:?}");
    }

    #[test]
    fn multiple_top_level_decls() {
        let out = run("color: blue;\nfont-size: 12px;");
        assert!(out.contains("._syaz13q2"), "got: {out:?}");
        assert!(out.contains("._1wyb1fwx"), "got: {out:?}");
    }

    #[test]
    fn important_changes_value_hash() {
        let out = run("color: red!important;\ncolor: red;");
        assert!(out.contains("._syaz1qpq"), "important hash: {out:?}");
        assert!(out.contains("._syaz5scu"), "non-important hash: {out:?}");
    }

    #[test]
    fn double_nesting_doubles_class_in_selector() {
        let out = run("&& { display: block; }");
        assert!(out.contains("._if291ule._if291ule"), "got: {out:?}");
    }

    #[test]
    fn nested_tag_rule_prepends_with_class() {
        let out = run("div { color: blue; }");
        assert!(out.contains("._65g013q2 div"), "got: {out:?}");
    }

    #[test]
    fn multi_selector_rule_atomicifies_each() {
        let out = run("div, span, li { color: blue; }");
        assert!(out.contains("._65g013q2 div"), "got: {out:?}");
        assert!(out.contains("._1tjq13q2 span"), "got: {out:?}");
        assert!(out.contains("._thoc13q2 li"), "got: {out:?}");
    }

    #[test]
    fn nested_pseudo_rule() {
        let out = run("&:hover, &:focus { color: blue; }");
        assert!(out.contains("._30l313q2:hover"), "got: {out:?}");
        assert!(out.contains("._f8pj13q2:focus"), "got: {out:?}");
    }

    #[test]
    fn at_rule_atomicify_inside() {
        let out = run("@media (min-width: 30rem) { display: block; }");
        assert!(out.contains("@media (min-width: 30rem)"), "got: {out:?}");
    }

    #[test]
    fn unknown_at_rule_throws() {
        let mut root = parse("@asdfghjkl state { div { color: blue; } }").unwrap();
        let err = atomicify_rules(&mut root, &mut AtomicifyRulesOpts::default()).unwrap_err();
        assert!(
            err.message.contains("Unknown at-rule '@asdfghjkl'"),
            "got: {err:?}"
        );
    }

    #[test]
    fn forbidden_at_rule_throws() {
        let mut root = parse("@charset \"utf-8\";").unwrap();
        let err = atomicify_rules(&mut root, &mut AtomicifyRulesOpts::default()).unwrap_err();
        assert!(
            err.message.contains("'@charset' cannot be used"),
            "got: {err:?}"
        );
    }

    #[test]
    fn nested_rule_throws() {
        let mut root = parse("div { div { color: red; } }").unwrap();
        let err = atomicify_rules(&mut root, &mut AtomicifyRulesOpts::default()).unwrap_err();
        assert!(err.message.contains("Nested rules need to be flattened first"));
    }

    #[test]
    fn comments_at_root_are_dropped() {
        let out = run("/* hello */");
        assert!(!out.contains("hello"), "got: {out:?}");
    }

    #[test]
    fn ignored_at_rule_passes_through_unchanged() {
        let css = "@font-face { font-family: \"Open Sans\"; }";
        let out = run(css);
        assert!(out.contains("@font-face"));
        assert!(out.contains("font-family"));
    }

    #[test]
    fn callback_collects_class_names_in_order() {
        let mut root = parse("color: blue;\nbackground: red;").unwrap();
        let mut opts = AtomicifyRulesOpts::default();
        atomicify_rules(&mut root, &mut opts).unwrap();
        assert_eq!(opts.class_names.len(), 2);
        assert!(opts.class_names[0].starts_with("_syaz"));
    }

    #[test]
    fn class_hash_prefix_invalid_errors() {
        let mut root = parse("color: blue;").unwrap();
        let mut opts = AtomicifyRulesOpts {
            class_hash_prefix: Some("123nope".to_string()),
            ..Default::default()
        };
        let err = atomicify_rules(&mut root, &mut opts).unwrap_err();
        assert!(err.message.contains("isn't a valid CSS identifier"), "got: {err:?}");
    }

    #[test]
    fn class_hash_prefix_changes_group_hash() {
        let a = run("color: blue;");
        let mut root = parse("color: blue;").unwrap();
        let mut opts = AtomicifyRulesOpts {
            class_hash_prefix: Some("foo".to_string()),
            ..Default::default()
        };
        atomicify_rules(&mut root, &mut opts).unwrap();
        let b = stringify(&root);
        assert_ne!(a, b, "prefixed: {a:?} unprefixed: {b:?}");
    }

    #[test]
    fn compression_map_swaps_class_in_selector() {
        let mut root = parse("color: blue;").unwrap();
        let mut map = IndexMap::new();
        // Top-level `color: blue;` → full class `_syaz13q2`. The
        // compression-map key omits the leading `_`.
        map.insert("syaz13q2".to_string(), "x".to_string());
        let mut opts = AtomicifyRulesOpts {
            class_name_compression_map: Some(map),
            ..Default::default()
        };
        atomicify_rules(&mut root, &mut opts).unwrap();
        let out = stringify(&root);
        assert!(out.contains(".x{"), "got: {out:?}");
        // Callback still sees full class.
        assert_eq!(opts.class_names, vec!["_syaz13q2".to_string()]);
    }

    #[test]
    fn no_op_on_blank_input() {
        assert_eq!(run(""), "");
    }
}
