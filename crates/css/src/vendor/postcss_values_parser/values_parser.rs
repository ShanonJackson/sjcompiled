//! Port of `postcss-values-parser/lib/ValuesParser.js`.
//!
//! Upstream extends postcss-core's Parser. We don't need the full Parser
//! base here — values strings have no `;` / `}` block structure. Instead we
//! consume the wrapped tokenizer output and classify each token into a Node
//! variant.

use super::nodes::*;
use super::nodes::at_word::AtWord;
use super::nodes::comment::Comment;
use super::nodes::func::Func;
use super::nodes::numeric::Numeric;
use super::nodes::operator::Operator;
use super::nodes::punctuation::Punctuation;
use super::nodes::quoted::Quoted;
use super::nodes::unicode_range::UnicodeRange;
use super::nodes::word::Word;
use super::tokenize::{get_tokens, VKind, VToken};
use once_cell::sync::Lazy;
use regex::Regex;

// Mirror upstream `Func.js:90-92`:
//   const reColorFunctions = /^(hsla?|hwb|lab|lch|rgba?)$/i;
//   const reVar = /^var$/i;
//   const reVarPrefix = /^--[^\s]+$/;
static RE_COLOR_FUNCTIONS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^(hsla?|hwb|lab|lch|rgba?)$").unwrap());
static RE_VAR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^var$").unwrap());
static RE_VAR_PREFIX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^--[^\s]+$").unwrap());

// Mirror upstream `Func.js:88` and the name validation in `Func.fromTokens`:
//
//   const cssFunctions = ['annotation', 'attr', 'blur', ...];
//   const vendorPrefixes = ['-webkit-', '-moz-', '-ms-', '-o-'];
//   const reFunctions = new RegExp(`^(${vendorPrefixes.join('|')})?(${cssFunctions.join('|')})`, 'i');
//
// And in `fromTokens`:
//   if (!reFunctions.test(node.name) && !/^[a-zA-Z\-\.]+$/.test(node.name)) {
//     // re-tokenize the name and back-push as Word + brackets
//   }
//
// `cssFunctions` list copied verbatim from upstream `Func.js:17-86`.
static RE_FUNCTIONS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)^(-webkit-|-moz-|-ms-|-o-)?",
        r"(annotation|attr|blur|brightness|calc|character-variant|circle|contrast|",
        r"cubic-bezier|dir|drop-shadow|element|ellipse|grayscale|hsl|hsla|hue-rotate|",
        r"image|inset|invert|lang|linear-gradient|matrix|matrix3d|minmax|not|nth-child|",
        r"nth-last-child|nth-last-of-type|nth-of-type|opacity|ornaments|perspective|",
        r"polygon|radial-gradient|rect|repeat|repeating-linear-gradient|",
        r"repeating-radial-gradient|rgb|rgba|rotate|rotatex|rotatey|rotatez|rotate3d|",
        r"saturate|scale|scalex|scaley|scalez|scale3d|sepia|skew|skewx|skewy|steps|",
        r"styleset|stylistic|swash|symbols|translate|translatex|translatey|translatez|",
        r"translate3d|url|var)"
    )).unwrap()
});

// Upstream `Func.fromTokens` fallback: `^[a-zA-Z\-\.]+$` (letters, dashes, dots).
// Notably DOES NOT permit digits or underscores.
static RE_FUNC_NAME_FALLBACK: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z\-.]+$").unwrap());

fn is_valid_func_name(name: &str) -> bool {
    RE_FUNCTIONS.is_match(name) || RE_FUNC_NAME_FALLBACK.is_match(name)
}

// Upstream `Operator.js:15` — the 10-char list. The 5 already retagged at the
// tokenizer layer (`*`, `-`, `%`, `+`, `/`) plus 5 that arrive as plain words:
// `=`, `<=`, `>=`, `<`, `>`. Used by the unknownWord classifier.
const OPERATOR_CHARS_FULL: &[&str] = &["+", "-", "/", "*", "%", "=", "<=", ">=", "<", ">"];

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
                        // Upstream `Func.js:195-196`: set isColor / isVar after children are built.
                        let is_color = RE_COLOR_FUNCTIONS.is_match(&pending.name);
                        let first_value = nodes.first().and_then(|n| match &n.kind {
                            NodeKind::Word(w) => Some(w.common.value.as_str()),
                            NodeKind::Numeric(n) => Some(n.common.value.as_str()),
                            NodeKind::Quoted(q) => Some(q.common.value.as_str()),
                            _ => None,
                        }).unwrap_or("");
                        let is_var = RE_VAR.is_match(&pending.name)
                            && !nodes.is_empty()
                            && RE_VAR_PREFIX.is_match(first_value);
                        let func = Func {
                            common: pending.common,
                            name: pending.name,
                            nodes,
                            is_color,
                            is_var,
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
                    // Path selector for tokens already classified as Comment by
                    // postcss-core. Always starts with `/*` in practice (postcss-core
                    // never emits an inline-comment kind), so this branch is
                    // defensive — see `Comment::is_inline_marker` doc.
                    let inline = Comment::is_inline_marker(&tok.value);
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
                    // Word can become: Func (followed by `(`), Comment (line comment
                    // `//...`), Numeric, UnicodeRange, Operator (10-char list),
                    // or Word.
                    let v = &tok.value;
                    let next_tok = tokens.get(idx + 1);
                    let next_is_open_paren = next_tok.map(|t| t.kind == VKind::OpenParen).unwrap_or(false);
                    let next_value: Option<&str> = next_tok.map(|t| t.value.as_str());

                    // Upstream `unknownWord` step 2: when a word's value is exactly
                    // `//`, collect tokens to next newline as a single inline Comment.
                    // Mirrors `Comment.tokenizeNext`. In practice the wrapped tokenizer
                    // splits `//` on the `/` chars so this rarely fires from a fresh
                    // word — but it does fire when an upstream pass back-pushes a
                    // raw `//` word.
                    if v == "//" {
                        let mut text = String::new();
                        let mut consumed = 0;
                        loop {
                            match tokens.get(idx + 1 + consumed) {
                                Some(t) if !t.value.contains('\n') => {
                                    text.push_str(&t.value);
                                    consumed += 1;
                                }
                                _ => break,
                            }
                        }
                        push_into(&mut stack_root, make_node(NodeKind::Comment(Comment {
                            common: common_for(tok),
                            text,
                            inline: true,
                            ..Default::default()
                        }), tok, &mut spaces_before));
                        idx += 1 + consumed;
                        continue;
                    }

                    // Upstream `Func.fromTokens` rejects names not matching either
                    // `reFunctions` (CSS function whitelist with optional vendor
                    // prefix) or `^[a-zA-Z\-\.]+$`. Invalid names fall through and
                    // the word is processed as a plain Word; the `(` is then handled
                    // by the OpenParen arm as Punctuation.
                    if next_is_open_paren && is_valid_func_name(v) {
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

                    // Upstream `unknownWord` step 5: `testWord` (escaped / hex / variable)
                    // is checked BEFORE Numeric/UnicodeRange. An escape-prefixed value
                    // like `\41` becomes Word, never Numeric.
                    if Word::test_word(v, next_value) {
                        let is_var = Word::is_variable_name(v);
                        let is_hex = Word::test_hex(v);
                        let is_color = Word::test_color(v);
                        let is_url = Word::test_url(v);
                        push_into(&mut stack_root, make_node(NodeKind::Word(Word {
                            common: common_for(tok),
                            is_variable: is_var,
                            is_hex,
                            is_color,
                            is_url,
                        }), tok, &mut spaces_before));
                        idx += 1;
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
                    } else if OPERATOR_CHARS_FULL.iter().any(|op| *op == v.as_str()) {
                        push_into(&mut stack_root, make_node(NodeKind::Operator(Operator {
                            common: common_for(tok),
                        }), tok, &mut spaces_before));
                    } else {
                        // Mirror upstream `Word.js:33-42`: set is_variable / is_hex /
                        // is_color / is_url at construction time.
                        let is_var = Word::is_variable_name(v);
                        let is_hex = Word::test_hex(v);
                        let is_color = Word::test_color(v);
                        let is_url = Word::test_url(v);
                        push_into(&mut stack_root, make_node(NodeKind::Word(Word {
                            common: common_for(tok),
                            is_variable: is_var,
                            is_hex,
                            is_color,
                            is_url,
                        }), tok, &mut spaces_before));
                    }
                    idx += 1;
                }
            }
        }

        // Unwind any unclosed function frames as Funcs with `unclosed = true`.
        // JS aborts via `parser.unclosedBracket(brackets)` (throws), so there is
        // no upstream reference for is_color/is_var on unclosed funcs. We still
        // tag is_color so the AST shape is consistent with the closed-func path
        // for recovery callers.
        while stack_func.len() > 1 {
            let nodes = stack_root.pop().unwrap();
            let pending = stack_func.pop().flatten().unwrap();
            let is_color = RE_COLOR_FUNCTIONS.is_match(&pending.name);
            let func = Func {
                common: pending.common,
                name: pending.name,
                nodes,
                unclosed: true,
                is_color,
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
    common: super::nodes::node::Common,
    name: String,
    raws_before: String,
    source_index: usize,
}

fn common_for(tok: &VToken) -> super::nodes::node::Common {
    super::nodes::node::Common {
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
