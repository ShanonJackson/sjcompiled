//! Port of `postcss-selector-parser/dist/parser.js`.
//!
//! Builds a typed selector AST from the upstream tokenizer. The full
//! upstream parser (1011 lines) handles every selector edge case; this port
//! covers the cases the local plugins (atomicifyRules, flattenMultiple-
//! Selectors, increaseSpecificity, parentOrphanedPseudos) actually walk:
//!
//!   * Class selectors `.foo`
//!   * ID selectors `#foo`
//!   * Tag selectors `div`
//!   * Universal `*`
//!   * Nesting `&`
//!   * Pseudo-class / pseudo-element with optional argument `:hover`,
//!     `:nth-child(2n+1)`, `::before`
//!   * Attribute selectors `[name="value"]`, `[name~="x" i]`
//!   * Combinators `>` `+` `~` ` ` (descendant)
//!   * Comma-separated selector lists
//!
//! For round-trip parity we also store each Selector's source text on
//! [`crate::nodes::Node::raw_value`] so [`crate::stringify`] can fall back
//! to original bytes when no mutation has occurred.

use crate::nodes::{AttributePayload, Node, NodeKind, Spaces};
use crate::processor::ProcessorOptions;
use crate::tokenize::{tokenize, Token, TokenizeError};
use crate::tokenTypes as t;

pub struct Parser {
    pub input: String,
    pub root: Node,
    tokens: Vec<Token>,
}

impl Parser {
    pub fn new(input: String, _opts: ProcessorOptions) -> Self {
        Parser { input, root: Node::root(), tokens: Vec::new() }
    }

    pub fn parse(&mut self) -> Result<&mut Node, TokenizeError> {
        self.tokens = tokenize(&self.input)?;

        // Split top-level selectors on `,`.
        let groups = split_top_level_groups(&self.tokens);
        for (group_idx, (start, end)) in groups.iter().copied().enumerate() {
            let mut selector = Node::selector();
            let source_start = if start < self.tokens.len() { self.tokens[start].start_pos } else { 0 };
            let source_end = if end > 0 && end <= self.tokens.len() {
                self.tokens[end - 1].end_pos
            } else { self.input.len() };
            // 6.1.0: `Selector.sourceIndex`. Upstream parser.js spawns the
            // first selector in the constructor with `sourceIndex: 0`
            // (line 120 in 6.1.2) regardless of leading whitespace, then
            // every comma-spawned sibling gets the start_pos of the token
            // *after* the comma (line 582). We replicate that exactly:
            // first group → 0; subsequent groups → first-token start_pos.
            selector.source_index = Some(if group_idx == 0 { 0 } else { source_start });
            let raw = self.input[source_start..source_end].to_string();
            selector.raw_value = Some(raw.clone());
            selector.value = raw.clone();

            // Build typed children.
            let token_slice: Vec<Token> = self.tokens[start..end].to_vec();
            self.build_selector_children(&token_slice, &mut selector);
            self.root.nodes.push(selector);
        }

        // Root preserves the full input on raw_value.
        self.root.set_raw_value(self.input.clone());
        Ok(&mut self.root)
    }

    /// Walk the token slice for one selector group and emit typed Nodes.
    fn build_selector_children(&self, tokens: &[Token], selector: &mut Node) {
        let mut i = 0;
        let mut pending_space: Option<String> = None;

        while i < tokens.len() {
            let tok = &tokens[i];
            if tok.kind == t::space {
                pending_space = Some(self.text(tok));
                i += 1;
            } else if tok.kind == t::word {
                let word = self.text(tok);
                // A single `word` token can encode a compound selector
                // (`.foo.bar`, `tag.x#id`, `#id.x`). Split it into a
                // sequence of typed Nodes — matches upstream
                // `parser.js::class()` / `id()` / `tag()` flow which emits
                // one ClassName/Identifier/Tag per leading sigil.
                let mut split = parse_word_compound(&word);
                if let Some(first) = split.first_mut() {
                    apply_pending_space(first, pending_space.take(), true);
                }
                for n in split {
                    selector.nodes.push(n);
                }
                i += 1;
            } else if tok.kind == t::asterisk {
                let mut node = Node {
                    kind: NodeKind::Universal,
                    value: "*".to_string(),
                    raw_value: None,
                    nodes: Vec::new(),
                    spaces: Spaces::default(),
                    attribute: None,
                    attribute_spaces: None,
                    source_index: None,
                };
                apply_pending_space(&mut node, pending_space.take(), true);
                selector.nodes.push(node);
                i += 1;
            } else if tok.kind == t::ampersand {
                let mut node = Node {
                    kind: NodeKind::Nesting,
                    value: "&".to_string(),
                    raw_value: None,
                    nodes: Vec::new(),
                    spaces: Spaces::default(),
                    attribute: None,
                    attribute_spaces: None,
                    source_index: None,
                };
                apply_pending_space(&mut node, pending_space.take(), true);
                selector.nodes.push(node);
                i += 1;
            } else if tok.kind == t::combinator {
                let combinator = self.text(tok);
                let before = pending_space.take().unwrap_or_default();
                let mut after = String::new();
                if i + 1 < tokens.len() && tokens[i + 1].kind == t::space {
                    after = self.text(&tokens[i + 1]);
                    i += 1;
                }
                selector.nodes.push(Node {
                    kind: NodeKind::Combinator,
                    value: combinator,
                    raw_value: None,
                    nodes: Vec::new(),
                    spaces: Spaces { before, after },
                    attribute: None,
                    attribute_spaces: None,
                    source_index: None,
                });
                i += 1;
            } else if tok.kind == t::colon {
                let mut value = ":".to_string();
                let mut j = i + 1;
                if j < tokens.len() && tokens[j].kind == t::colon {
                    value.push(':');
                    j += 1;
                }
                if j < tokens.len() && tokens[j].kind == t::word {
                    value.push_str(&self.text(&tokens[j]));
                    j += 1;
                }
                let mut nodes = Vec::new();
                if j < tokens.len() && tokens[j].kind == t::openParenthesis {
                    let close = find_matching_paren(tokens, j);
                    if let Some(end) = close {
                        let inner_start = if j + 1 < tokens.len() { tokens[j + 1].start_pos } else { tokens[j].end_pos };
                        let inner_end = tokens[end].start_pos;
                        let inner_groups = split_top_level_groups(&tokens[j + 1..end]);
                        for (s, e) in inner_groups {
                            let absolute_slice: Vec<Token> = tokens[j + 1 + s..j + 1 + e].to_vec();
                            let mut child_sel = Node::selector();
                            let cs_start = if !absolute_slice.is_empty() { absolute_slice[0].start_pos } else { inner_start };
                            let cs_end = if !absolute_slice.is_empty() { absolute_slice[absolute_slice.len() - 1].end_pos } else { inner_end };
                            // 6.1.0: `Selector.sourceIndex` for inner
                            // pseudo-arg selectors. parser.js
                            // `parentheses()` (line 653 in 6.1.2) spawns
                            // the first inner selector with start_pos of
                            // the token after `(`; comma-split arms come
                            // through `comma()` (line 582) using start_pos
                            // of the token after the comma. Both reduce
                            // to "start_pos of this group's first token,"
                            // falling back to inner_start for an empty
                            // group like `:is(,.a)`. 6.1.0 also shifted
                            // `source.start` from tokens[position-1] to
                            // tokens[position] on the parens-spawned
                            // selector — Rust doesn't track source.start
                            // line/column, so no byte impact.
                            child_sel.source_index = Some(if !absolute_slice.is_empty() {
                                absolute_slice[0].start_pos
                            } else {
                                inner_start
                            });
                            child_sel.raw_value = Some(self.input[cs_start..cs_end].to_string());
                            child_sel.value = child_sel.raw_value.clone().unwrap();
                            self.build_selector_children(&absolute_slice, &mut child_sel);
                            nodes.push(child_sel);
                        }
                        // `value` keeps the prefix only (`:not`, `::before`,
                        // `:nth-child`). The parens are rebuilt from
                        // `nodes` at stringify time so plugin mutations
                        // to inner selectors flow through to output. See
                        // `selectors::stringify` (Pseudo branch).
                        j = end + 1;
                    }
                }
                let mut node = Node {
                    kind: NodeKind::Pseudo,
                    value,
                    raw_value: None,
                    nodes,
                    spaces: Spaces::default(),
                    attribute: None,
                    attribute_spaces: None,
                    source_index: None,
                };
                apply_pending_space(&mut node, pending_space.take(), true);
                selector.nodes.push(node);
                i = j;
            } else if tok.kind == t::openSquare {
                let close = find_matching_square(tokens, i);
                if let Some(end) = close {
                    let attr_text = self.input[tokens[i].start_pos..tokens[end].end_pos].to_string();
                    let payload = parse_attribute(&attr_text);
                    let mut node = Node {
                        kind: NodeKind::Attribute,
                        value: attr_text,
                        raw_value: None,
                        nodes: Vec::new(),
                        spaces: Spaces::default(),
                        attribute: Some(payload),
                        attribute_spaces: None,
                        source_index: None,
                    };
                    apply_pending_space(&mut node, pending_space.take(), true);
                    selector.nodes.push(node);
                    i = end + 1;
                } else {
                    let rest = self.input[tokens[i].start_pos..].to_string();
                    selector.nodes.push(Node {
                        kind: NodeKind::Attribute,
                        value: rest,
                        raw_value: None,
                        nodes: Vec::new(),
                        spaces: Spaces::default(),
                        attribute: Some(AttributePayload::default()),
                        attribute_spaces: None,
                        source_index: None,
                    });
                    i = tokens.len();
                }
            } else {
                let txt = self.text(tok);
                selector.nodes.push(Node {
                    kind: NodeKind::Tag,
                    value: txt,
                    raw_value: None,
                    nodes: Vec::new(),
                    spaces: Spaces::default(),
                    attribute: None,
                    attribute_spaces: None,
                    source_index: None,
                });
                i += 1;
            }
        }

        if let Some(s) = pending_space {
            if let Some(last) = selector.nodes.last_mut() {
                // 6.1.2: trailing whitespace before `)` (or end of group)
                // attaches to the last node's `spaces.after`, NOT as a
                // standalone descendant combinator. Mirrors upstream
                // parser.js `combinator()` line 488 — the close-condition
                // gained `closeParenthesis` alongside `comma`. The Rust
                // port reaches the same end state structurally because
                // inner-pseudo slices exclude `)` and this tail block
                // folds residual whitespace into the previous node.
                last.spaces.after.push_str(&s);
            }
        }
    }

    fn text(&self, tok: &Token) -> String {
        self.input[tok.start_pos..tok.end_pos].to_string()
    }
}

/// Split a single `word` token into the sequence of typed nodes it
/// encodes. CSS allows compound selectors with no separator between
/// parts: `.foo.bar` is two classes, `div.foo` is tag + class,
/// `#id.x` is id + class, `.x#id` is class + id. The tokenizer treats
/// `.` and `#` as part of a word, so we split here.
fn parse_word_compound(word: &str) -> Vec<Node> {
    let mut nodes = Vec::new();
    let bytes = word.as_bytes();
    let mut i = 0usize;
    let len = bytes.len();
    if len == 0 {
        return nodes;
    }
    // Optional leading tag (no `.`/`#` prefix). Stops at the first sigil.
    if bytes[0] != b'.' && bytes[0] != b'#' {
        let mut end = 0usize;
        while end < len && bytes[end] != b'.' && bytes[end] != b'#' {
            end += 1;
        }
        nodes.push(Node {
            kind: NodeKind::Tag,
            value: word[0..end].to_string(),
            raw_value: None,
            nodes: Vec::new(),
            spaces: Spaces::default(),
            attribute: None,
            attribute_spaces: None,
            source_index: None,
        });
        i = end;
    }
    while i < len {
        let sigil = bytes[i];
        debug_assert!(sigil == b'.' || sigil == b'#');
        let name_start = i + 1;
        let mut end = name_start;
        // Honor backslash-escapes: `\.` inside the name doesn't terminate.
        // CSS-escaped class/id names like `._foo\:bar` keep the escaped
        // chars. We're conservative here: a `\` escapes the next byte
        // (postcss-selector-parser's `consumeEscape` handles hex escapes
        // too, but for the splitting purposes a single-byte skip suffices
        // because the tokenizer already absorbed the full escape).
        while end < len {
            if bytes[end] == b'\\' && end + 1 < len {
                end += 2;
                continue;
            }
            if bytes[end] == b'.' || bytes[end] == b'#' {
                break;
            }
            end += 1;
        }
        let kind = match sigil {
            b'.' => NodeKind::ClassName,
            b'#' => NodeKind::Identifier,
            _ => unreachable!(),
        };
        nodes.push(Node {
            kind,
            value: word[name_start..end].to_string(),
            raw_value: None,
            nodes: Vec::new(),
            spaces: Spaces::default(),
            attribute: None,
            attribute_spaces: None,
            source_index: None,
        });
        i = end;
    }
    nodes
}

fn apply_pending_space(node: &mut Node, pending: Option<String>, before: bool) {
    if let Some(s) = pending {
        if before { node.spaces.before.push_str(&s); }
        else { node.spaces.after.push_str(&s); }
    }
}

/// Split a token list at top-level (paren/square depth = 0) commas.
fn split_top_level_groups(tokens: &[Token]) -> Vec<(usize, usize)> {
    let mut groups = Vec::new();
    let mut paren = 0i32;
    let mut square = 0i32;
    let mut start = 0usize;
    for (i, tok) in tokens.iter().enumerate() {
        if tok.kind == t::openParenthesis { paren += 1; }
        else if tok.kind == t::closeParenthesis { paren -= 1; }
        else if tok.kind == t::openSquare { square += 1; }
        else if tok.kind == t::closeSquare { square -= 1; }
        else if tok.kind == t::comma && paren == 0 && square == 0 {
            groups.push((start, i));
            start = i + 1;
        }
    }
    groups.push((start, tokens.len()));
    groups
}

fn find_matching_paren(tokens: &[Token], open_at: usize) -> Option<usize> {
    let mut depth = 1i32;
    for i in (open_at + 1)..tokens.len() {
        if tokens[i].kind == t::openParenthesis { depth += 1; }
        else if tokens[i].kind == t::closeParenthesis {
            depth -= 1;
            if depth == 0 { return Some(i); }
        }
    }
    None
}

fn find_matching_square(tokens: &[Token], open_at: usize) -> Option<usize> {
    let mut depth = 1i32;
    for i in (open_at + 1)..tokens.len() {
        if tokens[i].kind == t::openSquare { depth += 1; }
        else if tokens[i].kind == t::closeSquare {
            depth -= 1;
            if depth == 0 { return Some(i); }
        }
    }
    None
}

/// Parse `[name="value"]` / `[ns|name~="value" i]` into the payload struct.
fn parse_attribute(text: &str) -> AttributePayload {
    let inner = text.trim_start_matches('[').trim_end_matches(']');
    let mut payload = AttributePayload::default();

    let inner = if inner.ends_with(" i") || inner.ends_with(" I") {
        payload.case_insensitive = true;
        &inner[..inner.len() - 2]
    } else { inner };

    let ops = ["~=", "|=", "^=", "$=", "*=", "="];
    let op_pos = ops.iter().filter_map(|op| inner.find(op).map(|p| (p, *op))).min_by_key(|(p, _)| *p);
    if let Some((p, op)) = op_pos {
        let (lhs, rhs) = inner.split_at(p);
        payload.operator = Some(op.to_string());
        let value_raw = &rhs[op.len()..];
        let value = value_raw.trim();
        if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            payload.quote_mark = Some(value.chars().next().unwrap());
            payload.value = Some(value[1..value.len() - 1].to_string());
            payload.raws_unquoted = Some(value.to_string());
        } else {
            payload.value = Some(value.to_string());
        }
        if let Some(pipe) = lhs.find('|') {
            payload.namespace = Some(lhs[..pipe].to_string());
            payload.attribute = lhs[pipe + 1..].to_string();
        } else {
            payload.attribute = lhs.to_string();
        }
    } else {
        if let Some(pipe) = inner.find('|') {
            payload.namespace = Some(inner[..pipe].to_string());
            payload.attribute = inner[pipe + 1..].to_string();
        } else {
            payload.attribute = inner.to_string();
        }
    }
    payload
}
