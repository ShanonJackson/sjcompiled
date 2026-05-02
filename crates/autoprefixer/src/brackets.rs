//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/brackets.js`.
//!
//! Tiny paren-aware nodes-tree parser. Round-trips byte-identically on
//! input that contains balanced parens.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Text(String),
    Group(Vec<Node>),
}

/// Parse string to nodes tree.
///
/// JS pushes a fresh `current = ['']` array on `(`, pops on `)`, and
/// concatenates ordinary chars onto the last string-slot of `current`.
/// We reproduce that byte-for-byte.
pub fn parse(s: &str) -> Vec<Node> {
    let mut stack: Vec<Vec<Node>> = vec![vec![Node::Text(String::new())]];

    for sym in s.chars() {
        if sym == '(' {
            let inner = vec![Node::Text(String::new())];
            stack.last_mut().unwrap().push(Node::Group(inner));
            // Re-borrow the freshly pushed group as the new current.
            let last_idx = stack.last().unwrap().len() - 1;
            let group_nodes = match stack.last_mut().unwrap().get_mut(last_idx).unwrap() {
                Node::Group(v) => std::mem::take(v),
                _ => unreachable!(),
            };
            stack.push(group_nodes);
            continue;
        }

        if sym == ')' {
            let popped = stack.pop().unwrap();
            // Replace the placeholder Group(empty) the parent currently holds
            // with the actual popped nodes, then push a fresh "" text slot
            // (mirroring JS `current.push('')`).
            let parent = stack.last_mut().unwrap();
            let last_idx = parent.len() - 1;
            parent[last_idx] = Node::Group(popped);
            parent.push(Node::Text(String::new()));
            continue;
        }

        // Append `sym` to the last text slot of the current frame.
        let frame = stack.last_mut().unwrap();
        if let Some(Node::Text(t)) = frame.last_mut() {
            t.push(sym);
        } else {
            frame.push(Node::Text(sym.to_string()));
        }
    }

    stack.into_iter().next().unwrap()
}

/// Generate output string by nodes tree.
pub fn stringify(ast: &[Node]) -> String {
    let mut result = String::new();
    for node in ast {
        match node {
            Node::Text(t) => result.push_str(t),
            Node::Group(g) => {
                result.push('(');
                result.push_str(&stringify(g));
                result.push(')');
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_simple() {
        let s = "linear-gradient(red, blue)";
        assert_eq!(stringify(&parse(s)), s);
    }

    #[test]
    fn round_trips_nested() {
        let s = "calc(1px + var(--x))";
        assert_eq!(stringify(&parse(s)), s);
    }

    #[test]
    fn round_trips_no_parens() {
        let s = "abc";
        assert_eq!(stringify(&parse(s)), s);
    }
}
