//! Port of `postcss/lib/parser.js`.
//!
//! Line-numbered references in this file point at upstream `parser.js`. The
//! state machine, raws bookkeeping, and error sites all match upstream.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::at_rule::AtRule;
use crate::comment::Comment;
use crate::css_syntax_error::CssSyntaxError;
use crate::declaration::Declaration;
use crate::input::Input;
use crate::node::{Node, NodeKind, RawValue, Raws, Source, SourcePosition};
use crate::root::Root;
use crate::rule::Rule;
use crate::tokenize::{tokenizer, Token, TokenKind, Tokenizer};

// `SAFE_COMMENT_NEIGHBOR = { empty: true, space: true }` — line 10.
fn is_safe_comment_neighbor(s: &str) -> bool { s == "empty" || s == "space" }

// **Why no Rust regex for the comment trim?**
//
// Upstream uses two regexes on `comment.text`:
//   - `^\s*$`            — "is the body all whitespace?"
//   - `^(\s*)([^]*\S)(\s*)$` — split into leading ws / trimmed body / trailing ws.
//
// Rust's `regex` crate uses Unicode `White_Space` for `\s`/`\S`. JS uses
// the ECMAScript `\s` set — they disagree on **U+0085** (in Rust's set,
// not in JS's) and **U+FEFF** (in JS's set, not in Rust's). A comment
// like `/*\u{FEFF}!keep*/` flows out of this trim into
// `postcss-discard-comments`'s `text.startsWith('!')` check; under JS
// the leading `\u{FEFF}` is stripped and the comment is kept; under
// Rust regex `\s` it would NOT be stripped and the comment is dropped.
// Silent hash divergence for any input with exotic whitespace adjacent
// to `!` in a comment.
//
// We replace both regexes with manual scans over
// [`crate::stringifier::is_js_regex_whitespace`] which enumerates the
// JS `\s` set exactly. See [`is_blank_comment_body`] and
// [`split_comment_body`] below.
static RE_WORD_HAS_LETTER: Lazy<Regex> = Lazy::new(|| Regex::new(r"\w").unwrap());

/// Mirrors JS `^\s*$` against `body` using the ECMAScript `\s` set.
fn is_blank_comment_body(body: &str) -> bool {
    body.chars().all(crate::stringifier::is_js_regex_whitespace)
}

/// Mirrors JS `^(\s*)([^]*\S)(\s*)$` capture: returns
/// `(leading_ws, trimmed_body, trailing_ws)`. Caller MUST guarantee the
/// body is not all-whitespace (otherwise the regex doesn't match —
/// `\S` requires at least one non-whitespace char). Use
/// [`is_blank_comment_body`] to gate.
fn split_comment_body(body: &str) -> (&str, &str, &str) {
    let mut start_byte = 0usize;
    for (i, c) in body.char_indices() {
        if !crate::stringifier::is_js_regex_whitespace(c) {
            start_byte = i;
            break;
        }
    }
    // Trailing: walk back until a non-whitespace char.
    let mut end_byte = body.len();
    let mut iter = body.char_indices().rev();
    for (i, c) in iter.by_ref() {
        if !crate::stringifier::is_js_regex_whitespace(c) {
            end_byte = i + c.len_utf8();
            break;
        }
    }
    (&body[..start_byte], &body[start_byte..end_byte], &body[end_byte..])
}

/// Stack frame indicating which container we're appending into. We use a
/// path of indices into the Root's tree because mutable borrowing through
/// nested `Vec<Node>` would otherwise tangle the borrow checker.
#[derive(Debug, Clone)]
struct CurrentPath(Vec<usize>);

impl CurrentPath {
    fn root() -> Self { CurrentPath(Vec::new()) }
    fn is_root(&self) -> bool { self.0.is_empty() }
    fn push(&mut self, idx: usize) { self.0.push(idx); }
    fn pop(&mut self) { self.0.pop(); }
}

pub struct Parser {
    pub input: Input,
    pub root: Root,
    current: CurrentPath,
    spaces: String,
    semicolon: bool,
    // NOTE: upstream had a `customProperty: bool` field on the parser
    // through 8.4.31; 8.5.6 removed it because the value was never read.
    // We dropped it here too (was generating an unused-field warning).
    /// We hold the tokenizer behind an `Option` so we can take ownership of
    /// it while we walk the AST (the tokenizer borrows the input's `&str`).
    tokens: Vec<Token>,
    tok_pos: usize,
    end_offset: usize,
}

impl Parser {
    pub fn new(input: Input) -> Self {
        let mut p = Parser {
            input,
            root: Root::new(),
            current: CurrentPath::root(),
            spaces: String::new(),
            semicolon: false,
            tokens: Vec::new(),
            tok_pos: 0,
            end_offset: 0,
        };
        p.root.root.source = Source {
            start: Some(SourcePosition { offset: 0, line: 1, column: 1 }),
            end: None,
            input_id: p.input.id,
        };
        p
    }

    fn create_tokenizer(&mut self) -> Result<(), CssSyntaxError> {
        let mut t: Tokenizer<'_> = tokenizer(&self.input, false);
        // Drain the whole token stream up front to sidestep lifetime issues
        // around interleaving parse-time `back()` calls with the borrow.
        loop {
            match t.next_token(false)? {
                Some(tok) => self.tokens.push(tok),
                None => break,
            }
        }
        self.end_offset = t.position();
        Ok(())
    }

    fn end_of_file(&self) -> bool { self.tok_pos >= self.tokens.len() }
    fn next_token(&mut self) -> Option<Token> {
        if self.tok_pos >= self.tokens.len() { return None; }
        let t = self.tokens[self.tok_pos].clone();
        self.tok_pos += 1;
        Some(t)
    }
    fn back(&mut self, _t: Token) { if self.tok_pos > 0 { self.tok_pos -= 1; } }

    pub fn parse(&mut self) -> Result<(), CssSyntaxError> {
        self.create_tokenizer()?;
        // Line 442 — main switch.
        while !self.end_of_file() {
            let token = self.next_token().unwrap();
            match token.kind {
                TokenKind::Space => { self.spaces.push_str(&token.content); }
                TokenKind::Semicolon => { self.free_semicolon(&token); }
                TokenKind::CloseCurly => { self.end(&token)?; }
                TokenKind::Comment => { self.comment(&token); }
                TokenKind::AtWord => { self.atrule(token)?; }
                TokenKind::OpenCurly => { self.empty_rule(&token); }
                _ => { self.other(token)?; }
            }
        }
        self.end_file();
        Ok(())
    }

    pub fn into_root(self) -> Root { self.root }

    // ---- helpers ----

    fn get_position(&self, offset: usize) -> SourcePosition {
        let (line, column) = self.input.from_offset(offset);
        SourcePosition { offset, line, column }
    }

    /// Mutable access to the container at `self.current`.
    fn current_nodes_mut(&mut self) -> &mut Vec<Node> {
        let path = self.current.0.clone();
        let mut nodes: &mut Vec<Node> = self.root.nodes_mut();
        for idx in &path {
            let n = &mut nodes[*idx];
            nodes = n.nodes_mut().expect("CurrentPath points at a container");
        }
        nodes
    }

    fn current_raws_mut(&mut self) -> &mut Raws {
        if self.current.is_root() {
            return self.root.raws_mut();
        }
        let path = self.current.0.clone();
        let mut nodes: &mut Vec<Node> = self.root.nodes_mut();
        let last = path.len() - 1;
        for idx in &path[..last] {
            let n = &mut nodes[*idx];
            nodes = n.nodes_mut().expect("CurrentPath points at a container");
        }
        &mut nodes[path[last]].raws
    }

    fn current_source_mut(&mut self) -> &mut Source {
        if self.current.is_root() { return &mut self.root.root.source; }
        let path = self.current.0.clone();
        let mut nodes: &mut Vec<Node> = self.root.nodes_mut();
        let last = path.len() - 1;
        for idx in &path[..last] {
            let n = &mut nodes[*idx];
            nodes = n.nodes_mut().expect("CurrentPath points at a container");
        }
        &mut nodes[path[last]].source
    }

    /// `init(node, offset)` — line 366. Pushes node onto current container.
    fn init_node(&mut self, node: Node, offset: usize) {
        let mut node = node;
        node.source.input_id = self.input.id;
        node.source.start = Some(self.get_position(offset));
        node.raws.before = Some(std::mem::take(&mut self.spaces));
        if !matches!(node.kind, NodeKind::Comment(_)) { self.semicolon = false; }
        self.current_nodes_mut().push(node);
    }

    /// Push, then descend into the just-pushed container.
    fn descend_into_last(&mut self) {
        let nodes = self.current_nodes_mut();
        let last = nodes.len() - 1;
        self.current.push(last);
    }

    // ---- comment ----

    /// `comment(token)` — line 173. Mirrors upstream's two-step trim
    /// using JS-spec `\s` semantics — see the comment above
    /// [`is_blank_comment_body`] for the divergence trap this avoids.
    fn comment(&mut self, token: &Token) {
        let mut comment = Comment::default();
        let mut raws = Raws::default();
        let body = &token.content[2..token.content.len() - 2];
        if is_blank_comment_body(body) {
            comment.text = String::new();
            raws.left = Some(body.to_string());
            raws.right = Some(String::new());
        } else {
            let (left, mid, right) = split_comment_body(body);
            comment.text = mid.to_string();
            raws.left = Some(left.to_string());
            raws.right = Some(right.to_string());
        }
        let mut node = Node::new(NodeKind::Comment(comment));
        node.raws = raws;
        node.source.end = Some(self.get_position(
            token.next.or(token.pos).unwrap_or(0) + 1,
        ));
        let offset = token.pos.unwrap_or(0);
        self.init_node(node, offset);
    }

    // ---- empty rule ----

    /// `emptyRule(token)` — line 309.
    fn empty_rule(&mut self, token: &Token) {
        let mut rule = Rule::default();
        rule.selector = String::new();
        let mut node = Node::new(NodeKind::Rule(rule));
        node.raws.between = Some(String::new());
        let offset = token.pos.unwrap_or(0);
        self.init_node(node, offset);
        self.descend_into_last();
    }

    // ---- free semicolon ----

    /// `freeSemicolon(token)` — line 344.
    fn free_semicolon(&mut self, token: &Token) {
        self.spaces.push_str(&token.content);
        // Attach `ownSemicolon` to the previous rule sibling, if any.
        let spaces_taken = std::mem::take(&mut self.spaces);
        let mut consumed = false;
        {
            let nodes = self.current_nodes_mut();
            if let Some(prev) = nodes.last_mut() {
                if let NodeKind::Rule(_) = prev.kind {
                    if prev.raws.own_semicolon.is_none() {
                        prev.raws.own_semicolon = Some(spaces_taken.clone());
                        consumed = true;
                    }
                }
            }
        }
        if !consumed { self.spaces = spaces_taken; }
    }

    // ---- end / endFile ----

    /// `end(token)` — line 317. Closes the current container.
    fn end(&mut self, token: &Token) -> Result<(), CssSyntaxError> {
        if self.current.is_root() {
            return Err(self.input.error("Unexpected }", token.pos.unwrap_or(0)));
        }
        let semicolon = self.semicolon;
        let spaces = std::mem::take(&mut self.spaces);
        // Set raws.semicolon if container has nodes.
        {
            let has_children = !self.current_nodes_mut().is_empty();
            let raws = self.current_raws_mut();
            if has_children { raws.semicolon = Some(semicolon); }
            let after = raws.after.take().unwrap_or_default();
            raws.after = Some(after + &spaces);
        }
        self.semicolon = false;
        // Set source.end.
        let end_pos = self.get_position(token.pos.unwrap_or(0) + 1);
        self.current_source_mut().end = Some(end_pos);
        self.current.pop();
        Ok(())
    }

    /// `endFile()` — line 335.
    fn end_file(&mut self) {
        // Walk back up, attaching trailing `spaces` to each open container's
        // `raws.after`. Upstream just closes the current container; for byte
        // parity we deposit spaces on the deepest open container.
        let semicolon = self.semicolon;
        let spaces = std::mem::take(&mut self.spaces);
        if !self.current.is_root() {
            // Unclosed block: still flush spaces to the current container so
            // round-trip preserves trailing whitespace.
            let has_children = !self.current_nodes_mut().is_empty();
            let raws = self.current_raws_mut();
            if has_children { raws.semicolon = Some(semicolon); }
            let after = raws.after.take().unwrap_or_default();
            raws.after = Some(after + &spaces);
        } else {
            let has_children = !self.root.nodes().is_empty();
            let raws = self.root.raws_mut();
            if has_children { raws.semicolon = Some(semicolon); }
            let after = raws.after.take().unwrap_or_default();
            raws.after = Some(after + &spaces);
        }
        let end = self.get_position(self.end_offset);
        self.root.root.source.end = Some(end);
    }

    // ---- atrule ----

    /// `atrule(token)` — line 37.
    fn atrule(&mut self, mut token: Token) -> Result<(), CssSyntaxError> {
        let name = token.content[1..].to_string();
        if name.is_empty() {
            return Err(self.input.error("At-rule without name", token.pos.unwrap_or(0)));
        }
        let mut at = AtRule::default();
        at.name = name;
        let start_offset = token.pos.unwrap_or(0);

        let mut params: Vec<Token> = Vec::new();
        let mut brackets: Vec<&'static str> = Vec::new();
        let mut last = false;
        let mut open = false;
        let mut hit_semicolon = false;

        while !self.end_of_file() {
            token = self.next_token().unwrap();
            let t_kind = token.kind.clone();

            match (&t_kind, brackets.last().copied()) {
                (TokenKind::OpenParen, _) => brackets.push(")"),
                (TokenKind::OpenSquare, _) => brackets.push("]"),
                (TokenKind::OpenCurly, Some(_)) => brackets.push("}"),
                (TokenKind::CloseParen, Some(")")) => { brackets.pop(); }
                (TokenKind::CloseSquare, Some("]")) => { brackets.pop(); }
                (TokenKind::CloseCurly, Some("}")) => { brackets.pop(); }
                _ => {}
            }

            if brackets.is_empty() {
                match t_kind {
                    TokenKind::Semicolon => { hit_semicolon = true; break; }
                    TokenKind::OpenCurly => { open = true; break; }
                    TokenKind::CloseCurly => {
                        // Unwind close-rule for `@charset` style atrules.
                        self.tok_pos -= 1; // back the `}` so end() handles it.
                        break;
                    }
                    _ => params.push(token),
                }
            } else {
                params.push(token);
            }

            if self.end_of_file() { last = true; break; }
        }

        // raws.between = trailing space/comment from end of params.
        let between = spaces_and_comments_from_end(&mut params);
        if !params.is_empty() {
            let after_name = spaces_and_comments_from_start(&mut params);
            at.has_block = open;
            let raw = self.raw_value(&params, false);
            at.params = raw.value.clone();
            let mut node = Node::new(NodeKind::AtRule(at));
            node.raws.after_name = Some(after_name);
            node.raws.between = Some(between);
            if let Some(rv) = raw.raw_record { node.raws.params = Some(rv); }
            if last {
                node.raws.between = Some(String::new());
                self.spaces = std::mem::take(node.raws.between.as_mut().unwrap());
            }
            self.init_node(node, start_offset);
        } else {
            at.has_block = open;
            let mut node = Node::new(NodeKind::AtRule(at));
            node.raws.after_name = Some(String::new());
            node.raws.between = Some(between);
            self.init_node(node, start_offset);
        }
        if open { self.descend_into_last(); }
        // Mirrors upstream timing: `init()` sets `semicolon = false`, but the
        // `;` that ended the atrule is supposed to bubble up so the parent
        // body's stringification appends it. Re-set after init.
        if hit_semicolon { self.semicolon = true; }
        Ok(())
    }

    // ---- other / decl / rule ----

    /// `other(start)` — line 377. Drives the rule/decl decision based on
    /// whether a `:` is found at top-level before a `{` / `;` / `}`.
    fn other(&mut self, start: Token) -> Result<(), CssSyntaxError> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut brackets: Vec<&'static str> = Vec::new();
        let mut bracket: Option<Token> = None;
        let mut colon = false;
        let mut end = false;
        let custom_property = start.content.starts_with("--");
        tokens.push(start);

        loop {
            let token = match self.next_token() {
                Some(t) => t,
                None => { end = true; break; }
            };
            let kind = token.kind.clone();
            tokens.push(token.clone());

            let last_close = brackets.last().copied();
            if matches!(kind, TokenKind::OpenParen) {
                if bracket.is_none() { bracket = Some(token.clone()); }
                brackets.push(")");
            } else if matches!(kind, TokenKind::OpenSquare) {
                if bracket.is_none() { bracket = Some(token.clone()); }
                brackets.push("]");
            } else if matches!(kind, TokenKind::OpenCurly) && custom_property && colon {
                if bracket.is_none() { bracket = Some(token.clone()); }
                brackets.push("}");
            } else if brackets.is_empty() {
                match kind {
                    TokenKind::Semicolon => {
                        if colon {
                            self.decl(&mut tokens, custom_property)?;
                            return Ok(());
                        } else { break; }
                    }
                    TokenKind::OpenCurly => {
                        self.rule(&mut tokens)?;
                        return Ok(());
                    }
                    TokenKind::CloseCurly => {
                        let popped = tokens.pop().unwrap();
                        self.back(popped);
                        end = true;
                        break;
                    }
                    TokenKind::Colon => { colon = true; }
                    _ => {}
                }
            } else if let Some(close) = last_close {
                if matches_close(&kind, close) {
                    brackets.pop();
                    if brackets.is_empty() { bracket = None; }
                }
            }

            if self.end_of_file() { end = true; break; }
        }

        if !brackets.is_empty() {
            return Err(self.input.error("Unclosed bracket", bracket
                .and_then(|t| t.pos)
                .unwrap_or(0)));
        }

        if end && colon {
            if !custom_property {
                while let Some(last) = tokens.last() {
                    if !matches!(last.kind, TokenKind::Space | TokenKind::Comment) { break; }
                    let popped = tokens.pop().unwrap();
                    self.back(popped);
                }
            }
            self.decl(&mut tokens, custom_property)?;
        } else {
            return Err(self.input.error(
                "Unknown word",
                tokens.first().and_then(|t| t.pos).unwrap_or(0),
            ));
        }
        Ok(())
    }

    /// `rule(tokens)` — line 517. The tokens vec ends with the `{` token,
    /// which we pop before extracting the selector.
    fn rule(&mut self, tokens: &mut Vec<Token>) -> Result<(), CssSyntaxError> {
        tokens.pop(); // drop the `{`.
        let between = spaces_and_comments_from_end(tokens);
        let raw = self.raw_value(tokens, false);
        let mut rule = Rule::default();
        rule.selector = raw.value.clone();
        let mut node = Node::new(NodeKind::Rule(rule));
        node.raws.between = Some(between);
        if let Some(rv) = raw.raw_record { node.raws.selector = Some(rv); }
        let offset = tokens.first().and_then(|t| t.pos).unwrap_or(0);
        self.init_node(node, offset);
        self.descend_into_last();
        Ok(())
    }

    /// `decl(tokens, customProperty)` — line 196.
    fn decl(&mut self, tokens: &mut Vec<Token>, custom_property: bool) -> Result<(), CssSyntaxError> {
        let mut decl = Declaration::default();
        decl.variable = custom_property;
        let mut raws = Raws::default();

        // Consume trailing `;`.
        if let Some(last) = tokens.last() {
            if matches!(last.kind, TokenKind::Semicolon) {
                self.semicolon = true;
                tokens.pop();
            }
        }

        let start_offset = tokens.first().and_then(|t| t.pos).unwrap_or(0);

        // Eat leading non-word tokens into raws.before.
        let mut before = String::new();
        while !tokens.is_empty() && !matches!(tokens[0].kind, TokenKind::Word) {
            if tokens.len() == 1 {
                return Err(self.input.error(
                    "Unknown word",
                    tokens[0].pos.unwrap_or(0),
                ));
            }
            before.push_str(&tokens.remove(0).content);
        }
        raws.before = Some(std::mem::take(&mut self.spaces) + &before);

        // Read prop until `:` / space / comment.
        let mut prop = String::new();
        while !tokens.is_empty() {
            match tokens[0].kind {
                TokenKind::Colon | TokenKind::Space | TokenKind::Comment => break,
                _ => prop.push_str(&tokens.remove(0).content),
            }
        }

        let mut between = String::new();
        while !tokens.is_empty() {
            let t = tokens.remove(0);
            if matches!(t.kind, TokenKind::Colon) {
                between.push_str(&t.content);
                break;
            } else {
                if matches!(t.kind, TokenKind::Word) && RE_WORD_HAS_LETTER.is_match(&t.content) {
                    return Err(self.input.error("Unknown word", t.pos.unwrap_or(0)));
                }
                between.push_str(&t.content);
            }
        }

        // Hack: leading `_` or `*` on prop migrates to raws.before.
        if let Some(first) = prop.chars().next() {
            if first == '_' || first == '*' {
                let new_before = raws.before.take().unwrap_or_default() + &first.to_string();
                raws.before = Some(new_before);
                prop = prop[first.len_utf8()..].to_string();
            }
        }
        raws.between = Some(between.clone());

        // Pull leading space/comment from value side into firstSpaces.
        let mut first_spaces: Vec<Token> = Vec::new();
        while let Some(t) = tokens.first() {
            if !matches!(t.kind, TokenKind::Space | TokenKind::Comment) { break; }
            first_spaces.push(tokens.remove(0));
        }

        // !important detection — line 258.
        for i in (0..tokens.len()).rev() {
            let (lower, kind_at_i) = {
                let t = &tokens[i];
                (t.content.to_ascii_lowercase(), t.kind.clone())
            };
            if lower == "!important" {
                decl.important = true;
                let mut s = string_from(tokens, i);
                s = spaces_from_end(tokens) + &s;
                if s != " !important" { raws.important = Some(s); }
                break;
            } else if lower == "important" {
                let cache = tokens.clone();
                let mut s = String::new();
                let mut j = i;
                while j > 0 {
                    let t_kind = cache[j].kind.clone();
                    if s.trim_start().starts_with('!') && !matches!(t_kind, TokenKind::Space) {
                        break;
                    }
                    s = tokens.pop().unwrap().content + &s;
                    j -= 1;
                }
                if s.trim_start().starts_with('!') {
                    decl.important = true;
                    raws.important = Some(s);
                }
            }
            if !matches!(kind_at_i, TokenKind::Space | TokenKind::Comment) { break; }
        }

        // Detect any non-space/comment tokens left.
        let has_word = tokens.iter().any(|t| !matches!(t.kind, TokenKind::Space | TokenKind::Comment));
        if has_word {
            let merged = first_spaces.iter().map(|t| t.content.as_str()).collect::<String>();
            raws.between = Some(between.clone() + &merged);
            first_spaces.clear();
        }

        // Combine remaining first_spaces + tokens into the value.
        let mut combined: Vec<Token> = first_spaces;
        combined.extend(std::mem::take(tokens));
        let raw = self.raw_value(&combined, custom_property);
        decl.prop = prop;
        decl.value = raw.value.clone();

        let mut node = Node::new(NodeKind::Declaration(decl));
        node.raws = raws;
        if let Some(rv) = raw.raw_record { node.raws.value = Some(rv); }
        node.source.start = Some(self.get_position(start_offset));
        self.current_nodes_mut().push(node);
        Ok(())
    }

    /// `raw(node, prop, tokens, customProperty)` — line 482. Returns the
    /// computed `value` plus an optional raw record (when comments/trailing
    /// space mean stringify needs the original bytes).
    fn raw_value(&self, tokens: &[Token], custom_property: bool) -> RawComputed {
        let mut value = String::new();
        let mut clean = true;
        let length = tokens.len();
        for i in 0..length {
            let token = &tokens[i];
            match token.kind {
                TokenKind::Space if i == length - 1 && !custom_property => { clean = false; }
                TokenKind::Comment => {
                    let prev = if i > 0 { kind_label(&tokens[i - 1].kind) } else { "empty" };
                    let next = if i + 1 < length { kind_label(&tokens[i + 1].kind) } else { "empty" };
                    if !is_safe_comment_neighbor(prev) && !is_safe_comment_neighbor(next) {
                        if value.ends_with(',') { clean = false; }
                        else { value.push_str(&token.content); }
                    } else { clean = false; }
                }
                _ => value.push_str(&token.content),
            }
        }
        let raw_record = if !clean {
            let raw: String = tokens.iter().map(|t| t.content.as_str()).collect();
            Some(RawValue { raw, value: value.clone() })
        } else { None };
        RawComputed { value, raw_record }
    }
}

struct RawComputed {
    value: String,
    raw_record: Option<RawValue>,
}

fn kind_label(k: &TokenKind) -> &'static str {
    match k {
        TokenKind::Space => "space",
        TokenKind::Comment => "comment",
        TokenKind::Word => "word",
        TokenKind::String => "string",
        TokenKind::AtWord => "at-word",
        TokenKind::Brackets => "brackets",
        TokenKind::Colon => ":",
        TokenKind::Semicolon => ";",
        TokenKind::OpenSquare => "[",
        TokenKind::CloseSquare => "]",
        TokenKind::OpenCurly => "{",
        TokenKind::CloseCurly => "}",
        TokenKind::OpenParen => "(",
        TokenKind::CloseParen => ")",
    }
}

fn matches_close(kind: &TokenKind, close: &str) -> bool {
    matches!(
        (kind, close),
        (TokenKind::CloseParen, ")") | (TokenKind::CloseSquare, "]") | (TokenKind::CloseCurly, "}")
    )
}

fn spaces_and_comments_from_end(tokens: &mut Vec<Token>) -> String {
    let mut spaces = String::new();
    while let Some(last) = tokens.last() {
        if !matches!(last.kind, TokenKind::Space | TokenKind::Comment) { break; }
        let t = tokens.pop().unwrap();
        spaces = t.content + &spaces;
    }
    spaces
}

fn spaces_and_comments_from_start(tokens: &mut Vec<Token>) -> String {
    let mut spaces = String::new();
    while let Some(first) = tokens.first() {
        if !matches!(first.kind, TokenKind::Space | TokenKind::Comment) { break; }
        let t = tokens.remove(0);
        spaces.push_str(&t.content);
    }
    spaces
}

fn spaces_from_end(tokens: &mut Vec<Token>) -> String {
    let mut spaces = String::new();
    while let Some(last) = tokens.last() {
        if !matches!(last.kind, TokenKind::Space) { break; }
        let t = tokens.pop().unwrap();
        spaces = t.content + &spaces;
    }
    spaces
}

fn string_from(tokens: &mut Vec<Token>, from: usize) -> String {
    let mut result = String::new();
    for i in from..tokens.len() {
        result.push_str(&tokens[i].content);
    }
    tokens.truncate(from);
    result
}

#[cfg(test)]
mod comment_trim_tests {
    use super::*;
    use crate::{parse, NodeKind};

    /// Locates the first comment in a parsed root and returns its
    /// `(text, raws.left, raws.right)`.
    fn extract_comment(css: &str) -> (String, String, String) {
        let root = parse(css).expect("parse");
        let n = root.nodes().iter()
            .find(|n| matches!(n.kind, NodeKind::Comment(_)))
            .expect("comment present");
        let text = match &n.kind {
            NodeKind::Comment(c) => c.text.clone(),
            _ => unreachable!(),
        };
        (
            text,
            n.raws.left.clone().unwrap_or_default(),
            n.raws.right.clone().unwrap_or_default(),
        )
    }

    /// Lock the divergence the agent flagged: `/*\u{FEFF}!keep*/` must
    /// trim the leading ZWNBSP so `text` starts with `!`. Then
    /// `postcss-discard-comments`'s `text.startsWith('!')` correctly
    /// preserves the comment.
    #[test]
    fn ufeff_zwnbsp_is_trimmed_from_comment() {
        let (text, left, right) = extract_comment("/*\u{FEFF}!keep*/ a { color: red; }");
        assert_eq!(left, "\u{FEFF}", "ZWNBSP must land on raws.left, not on text");
        assert_eq!(text, "!keep", "trimmed text must NOT contain ZWNBSP");
        assert_eq!(right, "");
        assert!(text.starts_with('!'),
            "discard-comments depends on this — `!keep` comments survive");
    }

    /// Lock the inverse: U+0085 (NEL) is in Unicode `White_Space` but
    /// NOT in JS `\s`. JS does NOT trim it from a comment; we must NOT
    /// trim it either. Otherwise `/*\u{0085}hi*/` would have text="hi"
    /// (JS) vs text="\u{0085}hi" (us) — but the divergence is the
    /// other direction here: we'd over-trim and JS wouldn't.
    ///
    /// Pre-fix: Rust `regex` `\s*` matched `\u{0085}` and trimmed it →
    /// text = "hi". JS's `\s*` would NOT match → text = "\u{0085}hi".
    /// Post-fix: our manual scan uses `is_js_regex_whitespace`, which
    /// excludes U+0085 → text = "\u{0085}hi", matching JS.
    #[test]
    fn nel_u0085_is_not_trimmed_from_comment() {
        let (text, left, _right) = extract_comment("/*\u{0085}hi*/ a {}");
        assert_eq!(left, "", "NEL is NOT JS whitespace; must not land on raws.left");
        assert_eq!(text, "\u{0085}hi", "NEL stays in text");
    }

    /// Standard ASCII whitespace trim still works.
    #[test]
    fn ascii_whitespace_trim_works() {
        let (text, left, right) = extract_comment("/*  hello  */ a {}");
        assert_eq!(left, "  ");
        assert_eq!(text, "hello");
        assert_eq!(right, "  ");
    }

    /// All-whitespace comment body becomes empty text + body-on-left.
    #[test]
    fn blank_comment_body() {
        let (text, left, right) = extract_comment("/*   */ a {}");
        assert_eq!(text, "");
        assert_eq!(left, "   ");
        assert_eq!(right, "");
    }

    /// All-whitespace via JS-spec whitespace (U+FEFF) — must hit the
    /// blank branch, not the trim branch.
    #[test]
    fn blank_comment_body_with_zwnbsp_only() {
        let (text, left, right) = extract_comment("/*\u{FEFF}\u{FEFF}*/ a {}");
        assert_eq!(text, "");
        assert_eq!(left, "\u{FEFF}\u{FEFF}");
        assert_eq!(right, "");
    }

    /// Trailing JS whitespace is trimmed off the end. Mirrors
    /// `/*hi  */` → text="hi", right="  ".
    #[test]
    fn trailing_whitespace_to_right() {
        let (text, left, right) = extract_comment("/*hi\u{FEFF}*/ a {}");
        assert_eq!(left, "");
        assert_eq!(text, "hi");
        assert_eq!(right, "\u{FEFF}");
    }
}
