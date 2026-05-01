//! Port of `postcss/lib/stringifier.js`.
//!
//! Line-numbered references in this file point at upstream `stringifier.js`.
//! Walks the AST and reconstructs the original byte sequence using each
//! node's `raws`. For our parity port we only need to handle "normal"
//! parser output — every node has its raws already populated, so we never
//! consult the `DEFAULT_RAW` table.

use crate::node::{Node, NodeKind};
use crate::root::Root;

/// Default raws — line 3 upstream. Used only when walking a node that was
/// constructed without going through the parser; the round-trip path on a
/// freshly-parsed Root never hits these.
fn default_raw(name: &str) -> &'static str {
    match name {
        "after" | "beforeClose" | "beforeComment" | "beforeDecl" | "beforeRule" => "\n",
        "beforeOpen" => " ",
        "colon" => ": ",
        "commentLeft" | "commentRight" => " ",
        "indent" => "    ",
        _ => "",
    }
}

pub struct Stringifier {
    out: String,
}

impl Stringifier {
    pub fn new() -> Self { Stringifier { out: String::new() } }

    pub fn finish(self) -> String { self.out }

    /// Top-level — stringifies a Root.
    pub fn stringify_root(&mut self, root: &Root) {
        self.body(&root.root);
        if let Some(after) = &root.root.raws.after { self.out.push_str(after); }
    }

    /// `stringify(node, semicolon)` — line 337.
    fn stringify(&mut self, node: &Node, semicolon: bool) {
        match &node.kind {
            NodeKind::Root(_) => self.stringify_node_root(node),
            NodeKind::AtRule(_) => self.atrule(node, semicolon),
            NodeKind::Rule(_) => self.rule(node),
            NodeKind::Declaration(_) => self.decl(node, semicolon),
            NodeKind::Comment(_) => self.comment(node),
        }
    }

    fn stringify_node_root(&mut self, node: &Node) {
        self.body(node);
        if let Some(after) = &node.raws.after { self.out.push_str(after); }
    }

    /// `atrule(node, semicolon)` — line 27.
    fn atrule(&mut self, node: &Node, semicolon: bool) {
        let at = match &node.kind { NodeKind::AtRule(a) => a, _ => unreachable!() };
        let mut name = String::from("@");
        name.push_str(&at.name);

        let params = if !at.params.is_empty() { self.raw_value_str(node, &at.params, node.raws.params.as_ref()) } else { String::new() };
        if let Some(after_name) = &node.raws.after_name { name.push_str(after_name); }
        else if !params.is_empty() { name.push(' '); }

        if at.has_block {
            self.block(node, &(name + &params));
        } else {
            let between = node.raws.between.clone().unwrap_or_default();
            self.out.push_str(&name);
            self.out.push_str(&params);
            self.out.push_str(&between);
            if semicolon { self.out.push(';'); }
        }
    }

    /// `rule(node)` — line 330.
    fn rule(&mut self, node: &Node) {
        let r = match &node.kind { NodeKind::Rule(r) => r, _ => unreachable!() };
        let selector = self.raw_value_str(node, &r.selector, node.raws.selector.as_ref());
        self.block(node, &selector);
        if let Some(own_semi) = &node.raws.own_semicolon { self.out.push_str(own_semi); }
    }

    /// `decl(node, semicolon)` — line 112.
    fn decl(&mut self, node: &Node, semicolon: bool) {
        let d = match &node.kind { NodeKind::Declaration(d) => d, _ => unreachable!() };
        let between = node.raws.between.clone().unwrap_or_else(|| ":".to_string());
        let value = self.raw_value_str(node, &d.value, node.raws.value.as_ref());
        let mut s = String::new();
        s.push_str(&d.prop);
        s.push_str(&between);
        s.push_str(&value);
        if d.important {
            s.push_str(node.raws.important.as_deref().unwrap_or(" !important"));
        }
        if semicolon { s.push(';'); }
        self.out.push_str(&s);
    }

    /// `comment(node)` — line 106.
    fn comment(&mut self, node: &Node) {
        let c = match &node.kind { NodeKind::Comment(c) => c, _ => unreachable!() };
        let left = node.raws.left.as_deref().unwrap_or(default_raw("commentLeft"));
        let right = node.raws.right.as_deref().unwrap_or(default_raw("commentRight"));
        self.out.push_str("/*");
        self.out.push_str(left);
        self.out.push_str(&c.text);
        self.out.push_str(right);
        self.out.push_str("*/");
    }

    /// `block(node, start)` — line 74.
    fn block(&mut self, node: &Node, start: &str) {
        let between = node.raws.between.clone().unwrap_or_default();
        self.out.push_str(start);
        self.out.push_str(&between);
        self.out.push('{');
        if let Some(children) = node.nodes() {
            if !children.is_empty() {
                self.body(node);
                if let Some(after) = &node.raws.after { self.out.push_str(after); }
            } else if let Some(after) = &node.raws.after {
                self.out.push_str(after);
            }
        }
        self.out.push('}');
    }

    /// `body(node)` — line 90.
    fn body(&mut self, node: &Node) {
        let children = match node.nodes() { Some(c) => c, None => return };
        // Find the last non-comment index.
        let mut last = if children.is_empty() { 0 } else { children.len() - 1 };
        while last > 0 {
            if !matches!(children[last].kind, NodeKind::Comment(_)) { break; }
            last -= 1;
        }
        let semicolon = node.raws.semicolon.unwrap_or(false);
        for (i, child) in children.iter().enumerate() {
            if let Some(before) = &child.raws.before {
                self.out.push_str(before);
            }
            self.stringify(child, last != i || semicolon);
        }
    }

    /// `rawValue(node, prop)` — line 315.
    fn raw_value_str(&self, _node: &Node, value: &str, raw: Option<&crate::node::RawValue>) -> String {
        if let Some(rv) = raw {
            if rv.value == value { return rv.raw.clone(); }
        }
        value.to_string()
    }
}

impl Default for Stringifier {
    fn default() -> Self { Self::new() }
}

pub fn stringify(root: &Root) -> String {
    let mut s = Stringifier::new();
    s.stringify_root(root);
    s.finish()
}
