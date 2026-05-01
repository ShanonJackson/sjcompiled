//! Port of `postcss-value-parser/lib/stringify.js`.

use crate::parse::{Node, NodeKind};

pub fn stringify(nodes: &[Node]) -> String {
    let mut out = String::new();
    for n in nodes { out.push_str(&stringify_node(n)); }
    out
}

fn stringify_node(node: &Node) -> String {
    match node.kind {
        NodeKind::Word | NodeKind::Space => node.value.clone(),
        NodeKind::String => {
            let q = node.quote.map(|c| c.to_string()).unwrap_or_default();
            if node.unclosed { format!("{q}{}", node.value) }
            else { format!("{q}{}{q}", node.value) }
        }
        NodeKind::Comment => {
            if node.unclosed { format!("/*{}", node.value) }
            else { format!("/*{}*/", node.value) }
        }
        NodeKind::Div => format!("{}{}{}", node.before, node.value, node.after),
        NodeKind::Function => {
            let inner = stringify(&node.nodes);
            let close = if node.unclosed { "" } else { ")" };
            format!("{}({}{}{}{}", node.value, node.before, inner, node.after, close)
        }
        NodeKind::UnicodeRange => node.value.clone(),
    }
}
