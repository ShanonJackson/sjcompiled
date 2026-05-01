//! Port of `postcss-value-parser/lib/parse.js`. Line-numbered references in
//! this file point at upstream `parse.js`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Word,
    Function,
    String,
    Div,
    Space,
    Comment,
    UnicodeRange,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub value: String,
    pub before: String,
    pub after: String,
    pub quote: Option<char>,
    pub unclosed: bool,
    pub nodes: Vec<Node>,
    pub source_index: usize,
    pub source_end_index: usize,
}

impl Node {
    fn new_word(value: String, source_index: usize, source_end_index: usize) -> Self {
        Node {
            kind: NodeKind::Word, value, before: String::new(), after: String::new(),
            quote: None, unclosed: false, nodes: Vec::new(), source_index, source_end_index,
        }
    }
}

const OPEN_PAREN: u8 = b'(';
const CLOSE_PAREN: u8 = b')';
const SINGLE_QUOTE: u8 = b'\'';
const DOUBLE_QUOTE: u8 = b'"';
const BACKSLASH: u8 = b'\\';
const SLASH: u8 = b'/';
const COMMA: u8 = b',';
const COLON: u8 = b':';
const STAR: u8 = b'*';
const U_LOWER: u8 = b'u';
const U_UPPER: u8 = b'U';
const PLUS: u8 = b'+';

fn is_unicode_range(s: &str) -> bool {
    if s.is_empty() { return false; }
    s.chars().all(|c| matches!(c,
        'a'..='f' | 'A'..='F' | '0'..='9' | '?' | '-'
    ))
}

/// Mirrors upstream `parse(input)` — returns a flat list of root tokens
/// (functions hold their children in `nodes`).
pub fn parse(input: &str) -> Vec<Node> {
    let mut value = input.to_string();
    let mut pos: usize = 0;
    let bytes_at = |s: &str, idx: usize| -> u8 { s.as_bytes().get(idx).copied().unwrap_or(0) };
    let mut code = bytes_at(&value, pos);
    let mut max = value.len();

    // Stack of nodes-vecs. Index 0 is the root output vec.
    let mut stack: Vec<Vec<Node>> = vec![Vec::new()];
    // Parallel stack of "this is a function frame" — when we close, we
    // attach the popped vec into the function node which lives one level up.
    // Function frames carry their parent's pending function-node info.
    let mut frame_func: Vec<Option<PendingFunc>> = vec![None];
    let mut balanced: i32 = 0;
    let mut name = String::new();
    let mut before = String::new();
    let mut after = String::new();

    while pos < max {
        if code <= 32 {
            // Whitespace run.
            let mut next = pos;
            loop {
                next += 1;
                code = bytes_at(&value, next);
                if !(code <= 32 && next < max) { break; }
            }
            let token = value[pos..next].to_string();
            let close_paren = code == CLOSE_PAREN && balanced > 0;
            // peek prev
            let prev_is_div = stack.last().and_then(|v| v.last()).map(|n| n.kind == NodeKind::Div).unwrap_or(false);
            let parent_is_calc = current_func_is_calc(&frame_func);
            let div_after = code == COMMA || code == COLON
                || (code == SLASH && bytes_at(&value, next + 1) != STAR
                    && (frame_func.last().and_then(|f| f.as_ref()).is_none()
                        || !parent_is_calc));
            if close_paren {
                after = token;
            } else if prev_is_div {
                let v = stack.last_mut().unwrap();
                let last_idx = v.len() - 1;
                let len_added = token.len();
                v[last_idx].after = token;
                v[last_idx].source_end_index += len_added;
            } else if div_after {
                before = token;
            } else {
                stack.last_mut().unwrap().push(Node {
                    kind: NodeKind::Space,
                    value: token,
                    before: String::new(),
                    after: String::new(),
                    quote: None,
                    unclosed: false,
                    nodes: Vec::new(),
                    source_index: pos,
                    source_end_index: next,
                });
            }
            pos = next;
        } else if code == SINGLE_QUOTE || code == DOUBLE_QUOTE {
            let quote_char = if code == SINGLE_QUOTE { '\'' } else { '"' };
            let quote_byte = code;
            let mut next = pos;
            let mut unclosed = false;
            loop {
                let mut escape = false;
                let from = next + 1;
                let found = value[from..].as_bytes().iter().position(|&b| b == quote_byte).map(|p| p + from);
                if let Some(found_pos) = found {
                    next = found_pos;
                    let mut escape_pos = next;
                    while escape_pos > 0 && value.as_bytes()[escape_pos - 1] == BACKSLASH {
                        escape_pos -= 1;
                        escape = !escape;
                    }
                    if !escape { break; }
                } else {
                    value.push(quote_char);
                    next = value.len() - 1;
                    max = value.len();
                    unclosed = true;
                    break;
                }
            }
            let inner = value[pos + 1..next].to_string();
            let source_end_index = if unclosed { next } else { next + 1 };
            stack.last_mut().unwrap().push(Node {
                kind: NodeKind::String,
                value: inner,
                before: String::new(),
                after: String::new(),
                quote: Some(quote_char),
                unclosed,
                nodes: Vec::new(),
                source_index: pos,
                source_end_index,
            });
            pos = next + 1;
            code = bytes_at(&value, pos);
        } else if code == SLASH && bytes_at(&value, pos + 1) == STAR {
            // Comments.
            let next = value[pos..].as_bytes().windows(2).position(|w| w == b"*/").map(|p| p + pos);
            let (next_idx, unclosed) = match next {
                Some(n) => (n, false),
                None => (value.len(), true),
            };
            let end_idx = if unclosed { next_idx } else { next_idx + 2 };
            let body_end = if unclosed { next_idx } else { next_idx };
            stack.last_mut().unwrap().push(Node {
                kind: NodeKind::Comment,
                value: value[pos + 2..body_end].to_string(),
                before: String::new(),
                after: String::new(),
                quote: None,
                unclosed,
                nodes: Vec::new(),
                source_index: pos,
                source_end_index: end_idx,
            });
            pos = if unclosed { value.len() } else { next_idx + 2 };
            code = bytes_at(&value, pos);
        } else if (code == SLASH || code == STAR) && current_func_is_calc(&frame_func) {
            let token = (code as char).to_string();
            stack.last_mut().unwrap().push(Node {
                kind: NodeKind::Word,
                value: token.clone(),
                before: String::new(),
                after: String::new(),
                quote: None,
                unclosed: false,
                nodes: Vec::new(),
                source_index: pos.saturating_sub(before.len()),
                source_end_index: pos + token.len(),
            });
            pos += 1;
            code = bytes_at(&value, pos);
        } else if code == SLASH || code == COMMA || code == COLON {
            let token = (code as char).to_string();
            let take_before = std::mem::take(&mut before);
            stack.last_mut().unwrap().push(Node {
                kind: NodeKind::Div,
                value: token.clone(),
                before: take_before,
                after: String::new(),
                quote: None,
                unclosed: false,
                nodes: Vec::new(),
                source_index: pos.saturating_sub(before.len()),
                source_end_index: pos + token.len(),
            });
            pos += 1;
            code = bytes_at(&value, pos);
        } else if code == OPEN_PAREN {
            // Whitespaces after open parentheses.
            let mut next = pos;
            loop {
                next += 1;
                code = bytes_at(&value, next);
                if !(code <= 32 && next < max) { break; }
            }
            let parens_open_pos = pos;
            let func_before = value[parens_open_pos + 1..next].to_string();
            let func_name = std::mem::take(&mut name);
            pos = next;

            if func_name == "url" && code != SINGLE_QUOTE && code != DOUBLE_QUOTE {
                next -= 1;
                let mut unclosed = false;
                loop {
                    let mut escape = false;
                    let from = next + 1;
                    let found = value[from..].as_bytes().iter().position(|&b| b == CLOSE_PAREN).map(|p| p + from);
                    if let Some(found_pos) = found {
                        next = found_pos;
                        let mut escape_pos = next;
                        while escape_pos > 0 && value.as_bytes()[escape_pos - 1] == BACKSLASH {
                            escape_pos -= 1;
                            escape = !escape;
                        }
                        if !escape { break; }
                    } else {
                        value.push(')');
                        next = value.len() - 1;
                        max = value.len();
                        unclosed = true;
                        break;
                    }
                }
                let mut whitespace_pos = next;
                loop {
                    if whitespace_pos == 0 { break; }
                    whitespace_pos -= 1;
                    code = bytes_at(&value, whitespace_pos);
                    if code > 32 { break; }
                }
                let mut nodes: Vec<Node> = Vec::new();
                let mut after_str = String::new();
                let func_source_end_index;
                if parens_open_pos < whitespace_pos {
                    if pos != whitespace_pos + 1 {
                        nodes.push(Node {
                            kind: NodeKind::Word,
                            value: value[pos..whitespace_pos + 1].to_string(),
                            before: String::new(), after: String::new(),
                            quote: None, unclosed: false, nodes: Vec::new(),
                            source_index: pos, source_end_index: whitespace_pos + 1,
                        });
                    }
                    if unclosed && whitespace_pos + 1 != next {
                        after_str = String::new();
                        nodes.push(Node {
                            kind: NodeKind::Space,
                            value: value[whitespace_pos + 1..next].to_string(),
                            before: String::new(), after: String::new(),
                            quote: None, unclosed: false, nodes: Vec::new(),
                            source_index: whitespace_pos + 1, source_end_index: next,
                        });
                        func_source_end_index = next;
                    } else {
                        after_str = value[whitespace_pos + 1..next].to_string();
                        func_source_end_index = next;
                    }
                } else {
                    after_str = String::new();
                    func_source_end_index = if unclosed { next } else { next + 1 };
                }
                let func_node = Node {
                    kind: NodeKind::Function,
                    value: func_name.clone(),
                    before: func_before,
                    after: after_str,
                    quote: None,
                    unclosed,
                    nodes,
                    source_index: parens_open_pos.saturating_sub(func_name.len()),
                    source_end_index: func_source_end_index,
                };
                stack.last_mut().unwrap().push(func_node);
                pos = next + 1;
                code = bytes_at(&value, pos);
            } else {
                balanced += 1;
                // Push function frame.
                let pending = PendingFunc {
                    name: func_name.clone(),
                    before: func_before,
                    source_index: parens_open_pos.saturating_sub(func_name.len()),
                };
                stack.push(Vec::new());
                frame_func.push(Some(pending));
            }
        } else if code == CLOSE_PAREN && balanced > 0 {
            pos += 1;
            code = bytes_at(&value, pos);
            let children = stack.pop().unwrap();
            let pending = frame_func.pop().flatten().expect("function frame");
            let take_after = std::mem::take(&mut after);
            balanced -= 1;
            stack.last_mut().unwrap().push(Node {
                kind: NodeKind::Function,
                value: pending.name,
                before: pending.before,
                after: take_after,
                quote: None,
                unclosed: false,
                nodes: children,
                source_index: pending.source_index,
                source_end_index: pos,
            });
        } else {
            // Words.
            let mut next = pos;
            loop {
                if code == BACKSLASH { next += 1; }
                next += 1;
                code = bytes_at(&value, next);
                let parent_is_calc = current_func_is_calc(&frame_func);
                if next >= max { break; }
                let stop = code <= 32
                    || code == SINGLE_QUOTE || code == DOUBLE_QUOTE
                    || code == COMMA || code == COLON || code == SLASH
                    || code == OPEN_PAREN
                    || (code == STAR && parent_is_calc)
                    || (code == SLASH && parent_is_calc)
                    || (code == CLOSE_PAREN && balanced > 0);
                if stop { break; }
            }
            let token = value[pos..next].to_string();
            if code == OPEN_PAREN {
                name = token;
            } else {
                let bytes = token.as_bytes();
                let is_uplus = bytes.len() > 2
                    && (bytes[0] == U_LOWER || bytes[0] == U_UPPER)
                    && bytes[1] == PLUS
                    && is_unicode_range(&token[2..]);
                if is_uplus {
                    stack.last_mut().unwrap().push(Node {
                        kind: NodeKind::UnicodeRange,
                        value: token, before: String::new(), after: String::new(),
                        quote: None, unclosed: false, nodes: Vec::new(),
                        source_index: pos, source_end_index: next,
                    });
                } else {
                    stack.last_mut().unwrap().push(Node::new_word(token, pos, next));
                }
            }
            pos = next;
        }
    }

    // Unwind any unclosed function frames.
    while frame_func.len() > 1 {
        let children = stack.pop().unwrap();
        let pending = frame_func.pop().flatten().expect("function frame");
        let len = value.len();
        stack.last_mut().unwrap().push(Node {
            kind: NodeKind::Function,
            value: pending.name,
            before: pending.before,
            after: String::new(),
            quote: None,
            unclosed: true,
            nodes: children,
            source_index: pending.source_index,
            source_end_index: len,
        });
    }

    stack.pop().unwrap()
}

#[derive(Debug, Clone)]
struct PendingFunc {
    name: String,
    before: String,
    source_index: usize,
}

fn current_func_is_calc(frames: &[Option<PendingFunc>]) -> bool {
    matches!(frames.last(), Some(Some(p)) if p.name == "calc")
}
