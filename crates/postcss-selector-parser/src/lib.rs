//! crates/postcss-selector-parser
//! Byte-for-byte Rust port of `postcss-selector-parser@6.1.2`.
//! See `crates/PARITY_VERSIONS.md`.
//!
//! Folder/file mapping (1:1 with `dist/`):
//!   - `index.js`        -> `src/lib.rs` (this file — public surface)
//!   - `parser.js`       -> `src/parser.rs`
//!   - `tokenize.js`     -> `src/tokenize.rs`
//!   - `tokenTypes.js`   -> `src/tokenTypes.rs`
//!   - `sortAscending.js`-> `src/sortAscending.rs`
//!   - `selectors/*.js`  -> `src/nodes.rs` + `src/selectors.rs`
//!   - `util/*.js`       -> `src/util.rs`
//!   - `processor.js`    -> `src/processor.rs`
//!
//! Module names mirror upstream JS file names verbatim (camelCase included).

#![allow(non_snake_case)]

pub mod parser;
pub mod tokenize;
pub mod tokenTypes;
pub mod sortAscending;
pub mod nodes;
pub mod selectors;
pub mod util;
pub mod processor;

pub use parser::Parser;
pub use processor::{Processor, ProcessorOptions};
pub use nodes::{
    walk_all, walk_attributes, walk_classes, walk_each, walk_pseudos,
    AttributePayload, AttributeSpaces, Node, NodeKind,
};
pub use selectors::stringify;

#[cfg(test)]
mod roundtrip_tests {
    use super::*;

    fn assert_roundtrip(sel: &str) {
        let proc = Processor::new();
        let out = proc.process(sel, |_root| {}).expect("parse ok");
        assert_eq!(out, sel, "selector round-trip mismatch\n  input:  {:?}\n  output: {:?}", sel, out);
    }

    #[test] fn class_only() { assert_roundtrip(".foo"); }
    #[test] fn id_only() { assert_roundtrip("#foo"); }
    #[test] fn descendant() { assert_roundtrip(".a .b"); }
    #[test] fn child_combinator() { assert_roundtrip(".a > .b"); }
    #[test] fn comma_list() { assert_roundtrip(".a, .b, .c"); }
    #[test] fn attribute_with_string() { assert_roundtrip("[data-x='y']"); }
    #[test] fn pseudo_function() { assert_roundtrip(":nth-child(2n+1)"); }
    #[test] fn nested_pseudo() { assert_roundtrip(":not(.a, .b)"); }
}

#[cfg(test)]
mod typed_ast_tests {
    use super::*;

    fn parse(sel: &str) -> Node {
        Processor::new().ast_sync(sel).expect("parse ok")
    }

    fn first_selector(root: &Node) -> &Node {
        root.nodes.first().expect("at least one selector group")
    }

    #[test]
    fn class_node_has_classname_kind() {
        let root = parse(".foo");
        let sel = first_selector(&root);
        let n = &sel.nodes[0];
        assert_eq!(n.kind, NodeKind::ClassName);
        assert_eq!(n.value, "foo");
    }

    #[test]
    fn id_node_has_identifier_kind() {
        let root = parse("#foo");
        let n = &first_selector(&root).nodes[0];
        assert_eq!(n.kind, NodeKind::Identifier);
        assert_eq!(n.value, "foo");
    }

    #[test]
    fn tag_selector() {
        let root = parse("div");
        let n = &first_selector(&root).nodes[0];
        assert_eq!(n.kind, NodeKind::Tag);
        assert_eq!(n.value, "div");
    }

    #[test]
    fn universal_selector() {
        let root = parse("*");
        let n = &first_selector(&root).nodes[0];
        assert_eq!(n.kind, NodeKind::Universal);
        assert_eq!(n.value, "*");
    }

    #[test]
    fn nesting_selector() {
        let root = parse("&");
        let n = &first_selector(&root).nodes[0];
        assert_eq!(n.kind, NodeKind::Nesting);
        assert_eq!(n.value, "&");
    }

    #[test]
    fn child_combinator_typed() {
        let root = parse(".a > .b");
        let nodes = &first_selector(&root).nodes;
        // .a + combinator(>) + .b
        let kinds: Vec<_> = nodes.iter().map(|n| n.kind.clone()).collect();
        assert!(kinds.contains(&NodeKind::Combinator));
        assert!(kinds.contains(&NodeKind::ClassName));
    }

    #[test]
    fn pseudo_class() {
        let root = parse("a:hover");
        let nodes = &first_selector(&root).nodes;
        assert_eq!(nodes[0].kind, NodeKind::Tag);
        assert_eq!(nodes[1].kind, NodeKind::Pseudo);
        assert_eq!(nodes[1].value, ":hover");
    }

    #[test]
    fn pseudo_element_double_colon() {
        let root = parse("a::before");
        let pseudo = &first_selector(&root).nodes[1];
        assert_eq!(pseudo.kind, NodeKind::Pseudo);
        assert!(pseudo.value.starts_with("::"));
    }

    #[test]
    fn comma_list_yields_multiple_selectors() {
        let root = parse(".a, .b, .c");
        assert_eq!(root.nodes.len(), 3);
        for s in &root.nodes { assert_eq!(s.kind, NodeKind::Selector); }
    }

    #[test]
    fn attribute_payload_parses_operator_and_value() {
        let root = parse(r#"[data-x="hi"]"#);
        let attr = &first_selector(&root).nodes[0];
        assert_eq!(attr.kind, NodeKind::Attribute);
        let payload = attr.attribute.as_ref().expect("attribute payload");
        assert_eq!(payload.attribute, "data-x");
        assert_eq!(payload.operator.as_deref(), Some("="));
        assert_eq!(payload.value.as_deref(), Some("hi"));
        assert_eq!(payload.quote_mark, Some('"'));
    }

    #[test]
    fn attribute_namespace() {
        let root = parse("[ns|data-x]");
        let attr = &first_selector(&root).nodes[0];
        let payload = attr.attribute.as_ref().expect("attribute payload");
        assert_eq!(payload.namespace.as_deref(), Some("ns"));
        assert_eq!(payload.attribute, "data-x");
    }

    #[test]
    fn attribute_case_insensitive_flag() {
        let root = parse(r#"[data-x="hi" i]"#);
        let attr = &first_selector(&root).nodes[0];
        let payload = attr.attribute.as_ref().expect("attribute payload");
        assert!(payload.case_insensitive);
    }

    #[test]
    fn mutate_classname_then_stringify() {
        // The atomicifyRules-style rename — change a class name and confirm
        // stringify emits the new value.
        let mut root = parse(".foo");
        // Mutate the ClassName node.
        if let Some(sel) = root.nodes.first_mut() {
            if let Some(class_node) = sel.nodes.first_mut() {
                class_node.set_value("bar".to_string());
                // Selector and Root must also drop their cached raw_value.
                sel.raw_value = None;
                sel.value = String::new();
            }
        }
        root.raw_value = None;
        let out = stringify(&root);
        assert_eq!(out, ".bar");
    }

    // ----------------------------------------------------------------
    // postcss-selector-parser surface for cssnano-postcss-minify-selectors
    // (added 2026-05-02). All tests below exercise additions that are
    // purely additive on top of the existing 6.1.2 port.
    // ----------------------------------------------------------------

    #[test]
    fn payload_dirty_rebuilds_unquoted_attribute() {
        // Mirrors `cssnano-postcss-minify-selectors`'s `attribute()`
        // reducer: trim attribute name + operator, clear quoteMark when
        // canUnquote, then zero out sub-spaces. Expected output:
        // `[data-x=hi]` — no quotes, no spaces.
        let mut root = parse(r#"[ data-x = "hi" ]"#);
        let sel = &mut root.nodes[0];
        let attr = &mut sel.nodes[0];
        attr.raw_value = None;
        sel.raw_value = None;
        let payload = attr.attribute.as_mut().unwrap();
        // Replicate the minify-selectors mutations.
        payload.attribute = payload.attribute.trim().to_string();
        if let Some(op) = payload.operator.take() {
            payload.operator = Some(op.trim().to_string());
        }
        payload.quote_mark = None;
        payload.dirty = true;
        attr.attribute_spaces = Some(AttributeSpaces::default());
        root.raw_value = None;
        let out = stringify(&root);
        assert_eq!(out, "[data-x=hi]");
    }

    #[test]
    fn payload_dirty_preserves_namespace_and_insensitive() {
        // `[ns|x="hi" i]` with case_insensitive — verify each sub-space
        // is honored. Set value.after = " " to match upstream's
        // defaultAttrConcat injection rule.
        let mut root = parse(r#"[ns|x="hi" i]"#);
        let sel = &mut root.nodes[0];
        let attr = &mut sel.nodes[0];
        attr.raw_value = None;
        sel.raw_value = None;
        attr.attribute.as_mut().unwrap().dirty = true;
        let mut spaces = AttributeSpaces::default();
        spaces.value.after = " ".to_string();
        attr.attribute_spaces = Some(spaces);
        root.raw_value = None;
        let out = stringify(&root);
        assert_eq!(out, r#"[ns|x="hi" i]"#);
    }

    #[test]
    fn walk_all_visits_every_descendant() {
        // walk_each filters by NodeKind; walk_all visits every descendant.
        // For `.a:not(.b, .c)` the descendants are: Selector, ClassName(.a),
        // Pseudo(:not), Selector, ClassName(.b), Selector, ClassName(.c) — 7.
        let mut root = parse(".a:not(.b, .c)");
        let mut walk_all_count = 0usize;
        nodes::walk_all(&mut root, &mut |_, _| { walk_all_count += 1; });
        let mut filtered_count = 0usize;
        nodes::walk_each(&mut root, &[NodeKind::ClassName], &mut |_, _| {
            filtered_count += 1;
        });
        assert!(walk_all_count > filtered_count,
            "walk_all should hit container kinds too (got {} vs filtered {})",
            walk_all_count, filtered_count);
        assert_eq!(filtered_count, 3, ".a/.b/.c → three ClassName matches");
    }

    #[test]
    fn process_sync_no_closure_round_trip() {
        let proc = Processor::new();
        let out = proc.process_sync(".foo > .bar").expect("parse ok");
        assert_eq!(out, ".foo > .bar");
    }

    #[test]
    fn un_dirty_attribute_emits_raw_text() {
        // dirty=false (default) → bracket text on `node.value` round-trips
        // verbatim, including unusual whitespace inside the brackets.
        let input = r#"[ data-x = "hi" ]"#;
        let proc = Processor::new();
        let out = proc.process_sync(input).expect("parse ok");
        assert_eq!(out, input);
    }
}
