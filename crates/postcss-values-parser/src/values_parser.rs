//! Port of `postcss-values-parser/lib/ValuesParser.js`.
//!
//! Upstream extends postcss-core's Parser. We don't need the full Parser
//! base here — values strings have no `;` / `}` block structure. Instead we
//! consume the wrapped tokenizer output and classify each token into a Node
//! variant.

use crate::nodes::*;
use crate::nodes::at_word::AtWord;
use crate::nodes::comment::Comment;
use crate::nodes::func::Func;
use crate::nodes::numeric::Numeric;
use crate::nodes::operator::Operator;
use crate::nodes::punctuation::Punctuation;
use crate::nodes::quoted::Quoted;
use crate::nodes::unicode_range::UnicodeRange;
use crate::nodes::word::Word;
use crate::tokenize::{get_tokens, VKind, VToken};

pub struct ValuesParser {
    pub input: String,
    pub root: Root,
}

impl ValuesParser {
    pub fn new(input: String) -> Self { ValuesParser { input, root: Root::new() } }

    pub fn parse(&mut self) {
        let tokens = get_tokens(&self.input);
        let mut spaces_before = String::new();
        let mut idx = 0usize;
        let mut stack_root: Vec<Vec<Node>> = vec![Vec::new()];
        let mut stack_func: Vec<Option<PendingFunc>> = vec![None];

        while idx < tokens.len() {
            let tok = &tokens[idx];
            match tok.kind {
                VKind::Space => {
                    spaces_before.push_str(&tok.value);
                    idx += 1;
                    continue;
                }
                VKind::OpenParen => {
                    // Unattached `(` opens an anonymous bracket grouping. Treat as Punctuation.
                    push_into(&mut stack_root, make_node(NodeKind::Punctuation(Punctuation {
                        common: common_for(tok),
                    }), tok, &mut spaces_before));
                    idx += 1;
                }
                VKind::CloseParen => {
                    if stack_func.len() > 1 {
                        let nodes = stack_root.pop().unwrap();
                        let pending = stack_func.pop().flatten().expect("func frame");
                        let func = Func {
                            common: pending.common,
                            name: pending.name,
                            nodes,
                            ..Default::default()
                        };
                        let mut node = Node {
                            kind: NodeKind::Func(func),
                            raws_before: pending.raws_before,
                            raws_after: std::mem::take(&mut spaces_before),
                            source_index: pending.source_index,
                        };
                        // Keep the `before` whitespace inside the function on raws_before of first child;
                        // raws_after of the function holds the space outside the closing `)`.
                        // For now we keep parity by capturing trailing spaces on the function itself.
                        if let NodeKind::Func(f) = &mut node.kind {
                            f.raws_after = std::mem::take(&mut node.raws_after);
                        }
                        stack_root.last_mut().unwrap().push(node);
                    } else {
                        push_into(&mut stack_root, make_node(NodeKind::Punctuation(Punctuation {
                            common: common_for(tok),
                        }), tok, &mut spaces_before));
                    }
                    idx += 1;
                }
                VKind::Comma => {
                    push_into(&mut stack_root, make_node(NodeKind::Punctuation(Punctuation {
                        common: common_for(tok),
                    }), tok, &mut spaces_before));
                    idx += 1;
                }
                VKind::Colon | VKind::Semicolon | VKind::OpenSquare | VKind::CloseSquare
                | VKind::OpenCurly | VKind::CloseCurly | VKind::Punctuation => {
                    push_into(&mut stack_root, make_node(NodeKind::Punctuation(Punctuation {
                        common: common_for(tok),
                    }), tok, &mut spaces_before));
                    idx += 1;
                }
                VKind::Operator => {
                    push_into(&mut stack_root, make_node(NodeKind::Operator(Operator {
                        common: common_for(tok),
                    }), tok, &mut spaces_before));
                    idx += 1;
                }
                VKind::String => {
                    let bytes = tok.value.as_bytes();
                    let q = if bytes.first().copied() == Some(b'\'') { '\'' } else { '"' };
                    let last = *bytes.last().unwrap_or(&0);
                    let unclosed = last as char != q || bytes.len() < 2;
                    push_into(&mut stack_root, make_node(NodeKind::Quoted(Quoted {
                        common: common_for(tok),
                        quote: q,
                        unclosed,
                    }), tok, &mut spaces_before));
                    idx += 1;
                }
                VKind::AtWord => {
                    let name = tok.value.trim_start_matches('@').to_string();
                    push_into(&mut stack_root, make_node(NodeKind::AtWord(AtWord {
                        common: common_for(tok),
                        name,
                    }), tok, &mut spaces_before));
                    idx += 1;
                }
                VKind::Comment => {
                    let inline = Comment::test_inline(&tok.value);
                    let body = if inline {
                        // `// rest`
                        tok.value.trim_start_matches("//").to_string()
                    } else {
                        tok.value.trim_start_matches("/*").trim_end_matches("*/").to_string()
                    };
                    push_into(&mut stack_root, make_node(NodeKind::Comment(Comment {
                        common: common_for(tok), text: body, inline, ..Default::default()
                    }), tok, &mut spaces_before));
                    idx += 1;
                }
                VKind::Word => {
                    // Word can become: Func (followed by `(`), Numeric, UnicodeRange, or Word.
                    let v = &tok.value;
                    let next_is_open_paren = tokens.get(idx + 1).map(|t| t.kind == VKind::OpenParen).unwrap_or(false);
                    if next_is_open_paren {
                        let common = common_for(tok);
                        let pending = PendingFunc {
                            common: common.clone(),
                            name: v.clone(),
                            raws_before: std::mem::take(&mut spaces_before),
                            source_index: tok.source_index,
                        };
                        stack_root.push(Vec::new());
                        stack_func.push(Some(pending));
                        idx += 2; // skip the word + the `(`
                        continue;
                    }
                    if Numeric::test(v) {
                        let (num, unit) = Numeric::split(v).unwrap_or((v.clone(), String::new()));
                        let mut common = common_for(tok);
                        common.value = num.clone();
                        push_into(&mut stack_root, make_node(NodeKind::Numeric(Numeric {
                            common, unit,
                        }), tok, &mut spaces_before));
                    } else if UnicodeRange::test(v) {
                        push_into(&mut stack_root, make_node(NodeKind::UnicodeRange(UnicodeRange {
                            common: common_for(tok),
                        }), tok, &mut spaces_before));
                    } else {
                        let is_var = Word::is_variable_name(v);
                        let is_hex = v.starts_with('#');
                        push_into(&mut stack_root, make_node(NodeKind::Word(Word {
                            common: common_for(tok),
                            is_variable: is_var,
                            is_hex,
                            ..Default::default()
                        }), tok, &mut spaces_before));
                    }
                    idx += 1;
                }
            }
        }

        // Unwind any unclosed function frames as Funcs with `unclosed = true`.
        while stack_func.len() > 1 {
            let nodes = stack_root.pop().unwrap();
            let pending = stack_func.pop().flatten().unwrap();
            let func = Func {
                common: pending.common,
                name: pending.name,
                nodes,
                unclosed: true,
                ..Default::default()
            };
            let node = Node {
                kind: NodeKind::Func(func),
                raws_before: pending.raws_before,
                raws_after: String::new(),
                source_index: pending.source_index,
            };
            stack_root.last_mut().unwrap().push(node);
        }

        let root_nodes = stack_root.pop().unwrap();
        // Trailing whitespace lives on the last node's raws_after; if there's
        // no last node, drop it on the root's raw_value.
        let mut root_nodes = root_nodes;
        if !spaces_before.is_empty() {
            if let Some(last) = root_nodes.last_mut() {
                last.raws_after.push_str(&spaces_before);
            } else {
                self.root.raw_value = Some(spaces_before.clone());
            }
        }
        self.root.nodes = root_nodes;
    }

    pub fn into_root(self) -> Root { self.root }
}

#[derive(Debug, Clone)]
struct PendingFunc {
    common: crate::nodes::node::Common,
    name: String,
    raws_before: String,
    source_index: usize,
}

fn common_for(tok: &VToken) -> crate::nodes::node::Common {
    crate::nodes::node::Common {
        value: tok.value.clone(),
        raws_before: String::new(),
        raws_after: String::new(),
        source_index: tok.source_index,
        source_end_index: tok.source_end_index,
    }
}

fn make_node(kind: NodeKind, tok: &VToken, spaces_before: &mut String) -> Node {
    Node {
        kind,
        raws_before: std::mem::take(spaces_before),
        raws_after: String::new(),
        source_index: tok.source_index,
    }
}

fn push_into(stack: &mut Vec<Vec<Node>>, node: Node) {
    stack.last_mut().unwrap().push(node);
}
