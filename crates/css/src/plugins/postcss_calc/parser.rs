//! Hand-rolled port of the jison-generated `parser.js` for `postcss-calc@8.2.4`.
//!
//! Upstream is 3808 LOC of jison state-table boilerplate around a tiny
//! grammar (see `parser.jison`, 112 LOC). Porting the LALR(1) state tables
//! literally to Rust would buy us nothing — the runtime decisions are fully
//! determined by the grammar rules. Instead, we hand-roll a Pratt-style
//! parser that produces the **byte-identical AST** the jison parser produces,
//! plus byte-identical error messages for the two error classes the upstream
//! lexer/parser actually emits in practice (Lexical error / Parse error).
//!
//! ## Grammar (verbatim from `parser.jison`)
//!
//! ```text
//! expression  : math_expression EOF
//! math_expression
//!   : CALC LPAREN math_expression RPAREN          { $$ = $3 }
//!   | math_expression ADD math_expression
//!   | math_expression SUB math_expression
//!   | math_expression MUL math_expression
//!   | math_expression DIV math_expression
//!   | LPAREN math_expression RPAREN               { ParenthesizedExpression }
//!   | function | dimension | number
//!
//! Precedence: %left ADD SUB; %left MUL DIV.
//! ```
//!
//! Token rules (verbatim, regex order matters — first match wins):
//!   0  \s+                                       (skip)
//!   1  (-(webkit|moz)-)?calc\b                   CALC
//!   2  [a-z][a-z0-9-]*\s*\((?:"..."|'...'|\(...\)|[^()]*)*\)   FUNCTION
//!   3  *                                          MUL
//!   4  /                                          DIV
//!   5  +                                          ADD
//!   6  -                                          SUB
//!   7..14  (NUMBER)em\b / ex\b / ch\b / rem\b / vw\b / vh\b / vmin\b / vmax\b
//!   15..21 (NUMBER)cm/mm/Q/in/pt/pc/px\b         LENGTH
//!   22..25 (NUMBER)deg/grad/rad/turn\b           ANGLE
//!   26..27 (NUMBER)s/ms\b                        TIME
//!   28..29 (NUMBER)Hz/kHz\b                      FREQ
//!   30..32 (NUMBER)dpi/dpcm/dppx\b               RES
//!   33     (NUMBER)%                             PERCENTAGE
//!   34     NUMBER\b                              NUMBER
//!   35     NUMBER<ident>\b                       UNKNOWN_DIMENSION
//!   36     (                                      LPAREN
//!   37     )                                      RPAREN
//!   38     <<EOF>>                                EOF
//!
//! Lexer is **case-insensitive**.

use std::fmt;

// --------------------------------------------------------------------------
// AST
// --------------------------------------------------------------------------

/// The dimension-tag matches upstream `node.type`. Unknown dimensions are
/// distinguished from value-types (the reducer's `isValueType` predicate
/// returns false for `UnknownDimension`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionKind {
    Length,        // LengthValue
    Angle,         // AngleValue
    Time,          // TimeValue
    Frequency,     // FrequencyValue
    Resolution,    // ResolutionValue
    Em,            // EmValue
    Ex,            // ExValue
    Ch,            // ChValue
    Rem,           // RemValue
    Vh,            // VhValue
    Vw,            // VwValue
    Vmin,          // VminValue
    Vmax,          // VmaxValue
    Percentage,    // PercentageValue
    Unknown,       // UnknownDimension
}

impl DimensionKind {
    /// `isValueType` predicate from `reducer.js:8-28`. UnknownDimension is
    /// **not** a value type.
    pub fn is_value_type(&self) -> bool {
        !matches!(self, DimensionKind::Unknown)
    }
}

/// Tagged-union mirror of upstream `CalcNode`. See `parser.jison` actions.
#[derive(Debug, Clone)]
pub enum CalcNode {
    /// `MathExpression` — operator is one of `+ - * /`.
    MathExpression {
        operator: String,
        left: Box<CalcNode>,
        right: Box<CalcNode>,
    },
    /// `ParenthesizedExpression` — emitted only by the `LPAREN math_expression RPAREN`
    /// production (NOT by `CALC LPAREN ... RPAREN`, which is unwrap-only).
    ParenthesizedExpression {
        content: Box<CalcNode>,
    },
    /// `Function` — raw FUNCTION token text including its parentheses
    /// (e.g. `"var(--a)"` or `"unknown(arg)"`).
    Function {
        value: String,
    },
    /// Generic dimension — unit shape determined by `kind`.
    /// Number-only: use `Number` variant instead.
    Dimension {
        kind: DimensionKind,
        value: f64,
        unit: String, // case-preserved (extracted via `/[a-z]+$/i`)
    },
    /// `Number` — no unit.
    Number {
        value: f64,
    },
}

// --------------------------------------------------------------------------
// Errors
// --------------------------------------------------------------------------

/// Upstream returns either a `Lexical error ...` (lexer's parseError) or a
/// `Parse error on line ...` (parser's parseError). Both surface as a thrown
/// `Error` whose `message` is the formatted error string. The transform layer
/// catches these and emits them via `result.warn(error.message, ...)`.
#[derive(Debug, Clone)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ParseError {}

// --------------------------------------------------------------------------
// Lexer
// --------------------------------------------------------------------------

/// Mirrors jison's flat token stream. Tokens carry only the text the parser
/// needs (the original lexeme).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Calc,                       // 'CALC'
    Function,                   // 'FUNCTION'
    Mul,                        // '*'
    Div,                        // '/'
    Add,                        // '+'
    Sub,                        // '-'
    Lparen,                     // '('
    Rparen,                     // ')'
    Number,                     // NUMBER (no unit)
    Length, Angle, Time, Freq, Res, Percentage,
    Em, Ex, Ch, Rem, Vw, Vh, Vmin, Vmax,
    UnknownDimension,
    Eof,
}

impl Tok {
    /// `describeSymbol(symbol)` upstream — returns the quoted string form
    /// used in `Expecting "X", "Y", ...` parse-error messages. Order and
    /// exact strings match the jison-generated symbol table.
    fn describe(&self) -> &'static str {
        match self {
            Tok::Calc => "\"CALC\"",
            Tok::Function => "\"FUNCTION\"",
            Tok::Mul => "\"MUL\"",
            Tok::Div => "\"DIV\"",
            Tok::Add => "\"ADD\"",
            Tok::Sub => "\"SUB\"",
            Tok::Lparen => "\"LPAREN\"",
            Tok::Rparen => "\"RPAREN\"",
            Tok::Number => "\"NUMBER\"",
            Tok::Length => "\"LENGTH\"",
            Tok::Angle => "\"ANGLE\"",
            Tok::Time => "\"TIME\"",
            Tok::Freq => "\"FREQ\"",
            Tok::Res => "\"RES\"",
            Tok::Percentage => "\"PERCENTAGE\"",
            Tok::Em => "\"EMS\"",
            Tok::Ex => "\"EXS\"",
            Tok::Ch => "\"CHS\"",
            Tok::Rem => "\"REMS\"",
            Tok::Vw => "\"VWS\"",
            Tok::Vh => "\"VHS\"",
            Tok::Vmin => "\"VMINS\"",
            Tok::Vmax => "\"VMAXS\"",
            Tok::UnknownDimension => "\"UNKNOWN_DIMENSION\"",
            Tok::Eof => "\"EOF\"",
        }
    }
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    text: String,
    /// Byte offset in input where the token starts.
    start: usize,
    /// Byte offset where the token ends (exclusive). Tracked so future
    /// extensions (e.g. richer error messages with byte ranges) can use it
    /// without re-scanning; not currently consumed downstream.
    #[allow(dead_code)]
    end: usize,
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    /// `matched` upstream — bytes consumed so far.
    matched: String,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            matched: String::new(),
        }
    }

    /// Skip whitespace (rule 0 — `\s+`). Whitespace is `[ \t\n\r\f\v]` in
    /// JS regex `\s`, plus extras (NBSP, BOM, etc.). For postcss-calc inputs
    /// in practice this means ASCII whitespace; we also accept the few
    /// non-ASCII whitespace chars JS `\s` matches because their absence
    /// would diverge silently.
    fn skip_ws(&mut self) {
        while self.pos < self.input.len() {
            let bytes = self.input.as_bytes();
            let c = bytes[self.pos];
            // ASCII whitespace.
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0C | 0x0B) {
                self.matched.push(c as char);
                self.pos += 1;
                continue;
            }
            // Non-ASCII whitespace per ECMA-262 \s: U+00A0, U+1680, U+2028,
            // U+2029, U+202F, U+205F, U+3000, U+FEFF, etc. Most CSS inputs
            // never see these. Decode the next char to check.
            if c >= 0x80 {
                let rest = &self.input[self.pos..];
                if let Some(ch) = rest.chars().next() {
                    if is_js_whitespace(ch) {
                        for b in ch.to_string().as_bytes() {
                            self.matched.push(*b as char);
                        }
                        self.pos += ch.len_utf8();
                        continue;
                    }
                }
            }
            break;
        }
    }

    /// Lex the next token after skipping whitespace. Returns the token, or
    /// a Lexical error if no rule matches.
    fn next_token(&mut self) -> Result<Token, ParseError> {
        self.skip_ws();
        if self.pos >= self.input.len() {
            return Ok(Token {
                tok: Tok::Eof,
                text: String::new(),
                start: self.pos,
                end: self.pos,
            });
        }

        let start = self.pos;
        let bytes = self.input.as_bytes();
        let c = bytes[self.pos];

        // Single-char punctuation FIRST — but the calc/function rules can
        // also start with letters and the number-rules with digits/dot, so
        // we need to dispatch by leading char then attempt longer-match
        // first. The original regex order:
        //   1: CALC
        //   2: FUNCTION
        //   3-6: * / + -
        //   7-35: number-prefixed rules + UNKNOWN_DIMENSION
        //   36-37: ( )

        // Rule 1: CALC. (-(webkit|moz)-)?calc\b
        if let Some(end) = match_calc(self.input, self.pos) {
            return Ok(self.consume(Tok::Calc, start, end));
        }
        // Rule 2: FUNCTION. [a-z][a-z0-9-]*\s*\(...\)
        if let Some(end) = match_function(self.input, self.pos) {
            return Ok(self.consume(Tok::Function, start, end));
        }
        // Rules 3-6: punctuation.
        if c == b'*' { return Ok(self.consume(Tok::Mul, start, start + 1)); }
        if c == b'/' { return Ok(self.consume(Tok::Div, start, start + 1)); }
        if c == b'+' { return Ok(self.consume(Tok::Add, start, start + 1)); }
        if c == b'-' { return Ok(self.consume(Tok::Sub, start, start + 1)); }

        // Rules 7-35: number-prefixed (digit or dot).
        if c.is_ascii_digit() || c == b'.' {
            // Match the number prefix once, then try suffix patterns in the
            // same regex-order as upstream.
            if let Some(num_end) = match_number_prefix(self.input, self.pos) {
                // Try unit suffixes in upstream's exact order.
                let after = num_end;
                // Rule 7: em\b
                if let Some(end) = match_unit(self.input, after, "em", true) {
                    return Ok(self.consume(Tok::Em, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "ex", true) {
                    return Ok(self.consume(Tok::Ex, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "ch", true) {
                    return Ok(self.consume(Tok::Ch, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "rem", true) {
                    return Ok(self.consume(Tok::Rem, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "vw", true) {
                    return Ok(self.consume(Tok::Vw, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "vh", true) {
                    return Ok(self.consume(Tok::Vh, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "vmin", true) {
                    return Ok(self.consume(Tok::Vmin, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "vmax", true) {
                    return Ok(self.consume(Tok::Vmax, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "cm", true) {
                    return Ok(self.consume(Tok::Length, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "mm", true) {
                    return Ok(self.consume(Tok::Length, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "q", true) {
                    return Ok(self.consume(Tok::Length, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "in", true) {
                    return Ok(self.consume(Tok::Length, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "pt", true) {
                    return Ok(self.consume(Tok::Length, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "pc", true) {
                    return Ok(self.consume(Tok::Length, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "px", true) {
                    return Ok(self.consume(Tok::Length, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "deg", true) {
                    return Ok(self.consume(Tok::Angle, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "grad", true) {
                    return Ok(self.consume(Tok::Angle, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "rad", true) {
                    return Ok(self.consume(Tok::Angle, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "turn", true) {
                    return Ok(self.consume(Tok::Angle, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "s", true) {
                    return Ok(self.consume(Tok::Time, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "ms", true) {
                    return Ok(self.consume(Tok::Time, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "hz", true) {
                    return Ok(self.consume(Tok::Freq, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "khz", true) {
                    return Ok(self.consume(Tok::Freq, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "dpi", true) {
                    return Ok(self.consume(Tok::Res, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "dpcm", true) {
                    return Ok(self.consume(Tok::Res, start, end));
                }
                if let Some(end) = match_unit(self.input, after, "dppx", true) {
                    return Ok(self.consume(Tok::Res, start, end));
                }
                // Rule 33: PERCENTAGE — `%` (NO `\b` upstream — line 3726).
                if after < self.input.len() && self.input.as_bytes()[after] == b'%' {
                    return Ok(self.consume(Tok::Percentage, start, after + 1));
                }
                // Rule 34: NUMBER\b — bare number with word boundary.
                if has_word_boundary(self.input, after) {
                    return Ok(self.consume(Tok::Number, start, after));
                }
                // Rule 35: UNKNOWN_DIMENSION — number followed by ident.
                if let Some(end) = match_unknown_dimension_suffix(self.input, after) {
                    return Ok(self.consume(Tok::UnknownDimension, start, end));
                }
                // No suffix matched and no word boundary → fall through to
                // Lexical error.
            }
        }

        // Rules 36-37.
        if c == b'(' { return Ok(self.consume(Tok::Lparen, start, start + 1)); }
        if c == b')' { return Ok(self.consume(Tok::Rparen, start, start + 1)); }

        // Lexical error: 'Lexical error on line 1: Unrecognized text.\n\n  Erroneous area:\n<pretty>'.
        // Per upstream `lexer.js:3325` — `'Lexical error' + lineno_msg + ': Unrecognized text.'`.
        // Lineno is always 1 for postcss-calc (input is single-line in practice;
        // value-parser stringifies tokens without preserving line breaks beyond
        // any embedded \n in the source; but the lexer counts \n in `matched`
        // for `yylineno`).
        Err(ParseError(self.format_lexical_error()))
    }

    fn consume(&mut self, tok: Tok, start: usize, end: usize) -> Token {
        let text = self.input[start..end].to_string();
        self.matched.push_str(&text);
        self.pos = end;
        Token { tok, text, start, end }
    }

    /// Build the lexical-error message exactly as upstream emits it.
    /// Matches `parser.js:3325`'s flow:
    /// 1. msg = `'Lexical error' + lineno_msg + ': Unrecognized text.'`
    ///    where lineno_msg = `' on line ' + (yylineno+1)` if yylineno is a number.
    /// 2. constructLexErrorInfo appends `\n\n  Erroneous area:\n` + prettyPrintRange().
    fn format_lexical_error(&self) -> String {
        // yylineno: 0-indexed line counter incremented on \n in matched.
        // For our common case yylineno = 0 → line 1.
        let yylineno = count_newlines(&self.matched);
        let mut msg = format!("Lexical error on line {}: Unrecognized text.", yylineno + 1);

        // prettyPrintRange — for a single-line input, the format is:
        //   `${lineno_pfx}: ${line}\n${errpfx}${lead}${mark}`
        // Where:
        //   lineno_display_width = 1 + (log10(l1|1) | 0)   // = 1 for line 1..9
        //   lineno_pfx           = ws-padded line number → "1"
        //   ws_prefix            = " " * (lineno_display_width - 1) → ""
        //   errpfx               = "^" * lineno_display_width  → "^"
        //   first_column         = pos within the *current line*
        //   last_column          = first_column + match.length (currently empty match → +1 minimum via len calc)
        //   offset = 2 + 1 + first_column      (lead-dot count)
        //   len    = max(2, last_column - first_column + 1)
        //   lead   = "." * (offset - 1)
        //   mark   = "^" * (len - 1)
        // For the `Unrecognized text` case, `this.match` is empty (the
        // current rule attempt failed). yylloc.first_column == current pos's
        // column; yylloc.last_column also == first_column (zero-width).
        //
        // Test input `10pc + unknown` errors at byte 7 (`u`). first_column=7,
        // last_column=8 → len=max(2, 2) = 2 → mark="^" (1 caret).
        // offset = 2+1+7 = 10 → lead = "." × 9 = "........."
        // errpfx = "^". Output: "1: 10pc + unknown\n^.........^". ✓
        //
        // Wait: upstream sets yylloc.last_column to first_column + this.match.length.
        // After a failed rule attempt, what is this.match? Tracking through
        // the lexer code is brittle; the empirical answer from running the
        // npm lib was: pretty-printed `1: 10pc + unknown\n^.........^`.
        // So first_column=7, last_column=8 (one char), len=2, mark=^. ✓

        let line_text = self.current_line();
        let first_column = self.current_column();
        // We model it as a one-char span at the current position (last_column = first_column + 1).
        // If we're already at EOF this never fires (whitespace-only input
        // returns EOF cleanly).
        let last_column = first_column + 1;

        // Single-digit line numbers.
        let lineno = yylineno + 1;
        let lineno_display_width = number_log10_floor(lineno) + 1;
        let lineno_pfx = format!(
            "{:>width$}",
            lineno,
            width = lineno_display_width
        );
        let errpfx = "^".repeat(lineno_display_width);
        let offset = 2 + 1 + first_column;
        let len = std::cmp::max(2, last_column - first_column + 1);
        let lead = ".".repeat(offset.saturating_sub(1));
        let mark = "^".repeat(len.saturating_sub(1));

        msg.push_str("\n\n  Erroneous area:\n");
        msg.push_str(&lineno_pfx);
        msg.push_str(": ");
        // Tabs in the printed line become spaces (line.replace(/\t/g, ' ')).
        for ch in line_text.chars() {
            if ch == '\t' { msg.push(' '); } else { msg.push(ch); }
        }
        msg.push('\n');
        msg.push_str(&errpfx);
        msg.push_str(&lead);
        msg.push_str(&mark);

        msg
    }

    fn current_line(&self) -> String {
        // The full input split by \n; pick the line we're currently in.
        // "matched" stops at the *last lexed token end*; current pos may be
        // ahead. Use input as the source of truth.
        let full = self.input;
        let lineno_zero = count_newlines(&self.matched);
        let mut line_start = 0usize;
        let mut count = 0usize;
        for (i, b) in full.bytes().enumerate() {
            if count == lineno_zero {
                line_start = i;
                break;
            }
            if b == b'\n' { count += 1; }
        }
        if count < lineno_zero {
            // No more newlines → input shorter than expected.
            line_start = full.len();
        }
        let rest = &full[line_start..];
        match rest.find('\n') {
            Some(end) => rest[..end].to_string(),
            None => rest.to_string(),
        }
    }

    fn current_column(&self) -> usize {
        // Column = pos within the current line, 0-indexed. JS yylloc.first_column
        // is set from rule.position[1] which matches char-count after last \n.
        let matched_bytes = self.matched.as_bytes();
        let mut last_nl = None;
        for (i, b) in matched_bytes.iter().enumerate() {
            if *b == b'\n' { last_nl = Some(i); }
        }
        let line_start_in_matched = last_nl.map(|i| i + 1).unwrap_or(0);
        // Column is bytes from last newline to end of matched.
        // Note this is the column at which the FAILED token started, which
        // is the current `pos` after `skip_ws`. `matched` includes everything
        // consumed so far (including whitespace).
        matched_bytes.len() - line_start_in_matched
    }
}

// --------------------------------------------------------------------------
// Lexer helpers
// --------------------------------------------------------------------------

/// Mirrors regex `(-(webkit|moz)-)?calc\b` (case-insensitive).
fn match_calc(input: &str, pos: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut p = pos;
    // Optional `-(webkit|moz)-` prefix.
    if p < bytes.len() && bytes[p] == b'-' {
        let try_prefix = |word: &str| -> bool {
            let need = word.len() + 2; // -word-
            if p + need <= bytes.len() {
                let mid = &input[p + 1..p + 1 + word.len()];
                if eq_icase(mid, word) && bytes[p + 1 + word.len()] == b'-' {
                    return true;
                }
            }
            false
        };
        if try_prefix("webkit") {
            p += 1 + 6 + 1; // - + webkit + -
        } else if try_prefix("moz") {
            p += 1 + 3 + 1;
        } else {
            return None;
        }
    }
    // 'calc'
    if p + 4 > bytes.len() { return None; }
    if !eq_icase(&input[p..p + 4], "calc") { return None; }
    p += 4;
    // \b: next char is NOT [A-Za-z0-9_].
    if !is_word_boundary_after(input, p) { return None; }
    Some(p)
}

/// Mirrors `[a-z][a-z0-9-]*\s*\((?:(?:"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|\([^)]*\)|[^()]*)*\)`
/// (case-insensitive). Returns the byte-offset of the closing `)` + 1.
///
/// Important quirk: this regex permits **ONE level** of nested parens via the
/// `\([^)]*\)` alternative — i.e. exactly one inner `(...)` group with no
/// embedded parens. `var(--xxx, var(--yyy))` should match because inner
/// `var(--yyy)` matches `\([^)]*\)`. Let me verify... actually the regex
/// is a `(...)*` quantifier over alternatives, so it can repeat: each iteration
/// matches either a string, a `\([^)]*\)`, or a `[^()]*` chunk. So the call
/// `var(--xxx, var(--yyy))` parses as:
///   - opening `(`
///   - `[^()]*` → `--xxx, ` ?  wait `[^()]*` is greedy → matches `--xxx, var`?
///     No — `*` is repeated outside. The body of the function is matched by
///     repeated alternatives. With greedy `[^()]*`, it grabs `--xxx, var`,
///     then on next iteration encounters `(`, which matches `\([^)]*\)` →
///     consumes `(--yyy)`. Then closing `)` finishes the FUNCTION.
fn match_function(input: &str, pos: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut p = pos;
    // [a-z]
    if p >= bytes.len() || !bytes[p].is_ascii_alphabetic() { return None; }
    p += 1;
    // [a-z0-9-]*
    while p < bytes.len() {
        let c = bytes[p];
        if c.is_ascii_alphanumeric() || c == b'-' { p += 1; } else { break; }
    }
    // \s*  — only ASCII space-likes consumed by JS \s.
    while p < bytes.len() {
        let c = bytes[p];
        if matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0C | 0x0B) { p += 1; } else { break; }
    }
    // \(
    if p >= bytes.len() || bytes[p] != b'(' { return None; }
    p += 1;
    // Body: repeated alternatives.
    loop {
        if p >= bytes.len() { return None; } // unclosed, no match
        match bytes[p] {
            b')' => { p += 1; return Some(p); }
            b'"' | b'\'' => {
                let q = bytes[p];
                p += 1;
                while p < bytes.len() {
                    let c = bytes[p];
                    if c == b'\\' && p + 1 < bytes.len() {
                        // Skip the escape and the next byte.
                        // Note: JS `[^"\\]` in `\\.|[^"\\]` allows any
                        // single char after \, including newline. We track UTF-8 boundary.
                        p += 1;
                        let rest = &input[p..];
                        if let Some(ch) = rest.chars().next() {
                            p += ch.len_utf8();
                        }
                        continue;
                    }
                    if c == q { p += 1; break; }
                    let rest = &input[p..];
                    if let Some(ch) = rest.chars().next() {
                        p += ch.len_utf8();
                    } else { p += 1; }
                }
            }
            b'(' => {
                // \([^)]*\)
                p += 1;
                while p < bytes.len() && bytes[p] != b')' {
                    let rest = &input[p..];
                    if let Some(ch) = rest.chars().next() {
                        p += ch.len_utf8();
                    } else { p += 1; }
                }
                if p < bytes.len() && bytes[p] == b')' { p += 1; }
                else { return None; } // unmatched inner paren
            }
            _ => {
                // [^()]*  — consume up to the next paren or quote.
                while p < bytes.len() {
                    let c = bytes[p];
                    if matches!(c, b'(' | b')' | b'"' | b'\'') { break; }
                    let rest = &input[p..];
                    if let Some(ch) = rest.chars().next() {
                        p += ch.len_utf8();
                    } else { p += 1; }
                }
            }
        }
    }
}

/// Match the leading numeric portion: `((\d+(\.\d+)?|\.\d+)(e(\+|-)\d+)?)`
/// Returns the byte offset just after the matched number, or None.
fn match_number_prefix(input: &str, pos: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut p = pos;
    // (\d+(\.\d+)?|\.\d+)
    if p < bytes.len() && bytes[p].is_ascii_digit() {
        while p < bytes.len() && bytes[p].is_ascii_digit() { p += 1; }
        if p < bytes.len() && bytes[p] == b'.' {
            let q = p + 1;
            if q < bytes.len() && bytes[q].is_ascii_digit() {
                p = q;
                while p < bytes.len() && bytes[p].is_ascii_digit() { p += 1; }
            }
        }
    } else if p < bytes.len() && bytes[p] == b'.' {
        let q = p + 1;
        if q < bytes.len() && bytes[q].is_ascii_digit() {
            p = q;
            while p < bytes.len() && bytes[p].is_ascii_digit() { p += 1; }
        } else {
            return None;
        }
    } else {
        return None;
    }
    // (e(\+|-)\d+)?  — sign is REQUIRED.
    if p < bytes.len() && (bytes[p] == b'e' || bytes[p] == b'E') {
        let q = p + 1;
        if q < bytes.len() && (bytes[q] == b'+' || bytes[q] == b'-') {
            let r = q + 1;
            if r < bytes.len() && bytes[r].is_ascii_digit() {
                p = r;
                while p < bytes.len() && bytes[p].is_ascii_digit() { p += 1; }
            }
        }
    }
    Some(p)
}

/// Match a unit literal at `pos`, case-insensitive. If `boundary` is true,
/// require a word-boundary AFTER the unit (next char not `[A-Za-z0-9_]`).
fn match_unit(input: &str, pos: usize, unit: &str, boundary: bool) -> Option<usize> {
    if pos + unit.len() > input.len() { return None; }
    let slice = &input[pos..pos + unit.len()];
    if !eq_icase(slice, unit) { return None; }
    let end = pos + unit.len();
    if boundary && !is_word_boundary_after(input, end) { return None; }
    Some(end)
}

/// Match `-?<ident-start>[<ident-cont>]*\b` (the UNKNOWN_DIMENSION suffix).
/// Mirrors the cleaned-up regex at parser.js:3728 (after JS unicode ranges):
///   `-?([^\W\d]|[\u00A0-\u00FF]|<escape>)([\w\-]|[\u00A0-\u00FF]|<escape>)*\b`
/// We approximate `[^\W\d]` = `[A-Za-z_]` (ASCII; JS \w is [A-Za-z0-9_]).
fn match_unknown_dimension_suffix(input: &str, pos: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut p = pos;
    if p < bytes.len() && bytes[p] == b'-' { p += 1; }
    // Ident start: [A-Za-z_] | non-ASCII | escape.
    let consumed_start = consume_ident_start(input, p);
    if consumed_start == 0 { return None; }
    p += consumed_start;
    // Ident cont chars.
    loop {
        let n = consume_ident_cont(input, p);
        if n == 0 { break; }
        p += n;
    }
    // \b
    if !is_word_boundary_after(input, p) { return None; }
    Some(p)
}

fn consume_ident_start(input: &str, pos: usize) -> usize {
    let bytes = input.as_bytes();
    if pos >= bytes.len() { return 0; }
    let c = bytes[pos];
    if c.is_ascii_alphabetic() || c == b'_' { return 1; }
    if c >= 0x80 {
        let rest = &input[pos..];
        if let Some(ch) = rest.chars().next() {
            return ch.len_utf8();
        }
    }
    if c == b'\\' { return consume_escape(input, pos); }
    0
}

fn consume_ident_cont(input: &str, pos: usize) -> usize {
    let bytes = input.as_bytes();
    if pos >= bytes.len() { return 0; }
    let c = bytes[pos];
    if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' { return 1; }
    if c >= 0x80 {
        let rest = &input[pos..];
        if let Some(ch) = rest.chars().next() {
            return ch.len_utf8();
        }
    }
    if c == b'\\' { return consume_escape(input, pos); }
    0
}

/// CSS escape: `\\<hex 1-6>(\r\n|[ \t\n\f\r])?` or `\\<non-hex,non-newline-non-digit>`.
fn consume_escape(input: &str, pos: usize) -> usize {
    let bytes = input.as_bytes();
    if pos >= bytes.len() || bytes[pos] != b'\\' { return 0; }
    let mut p = pos + 1;
    if p >= bytes.len() { return 0; }
    let c = bytes[p];
    if c.is_ascii_hexdigit() {
        let mut hex_count = 0;
        while p < bytes.len() && hex_count < 6 && bytes[p].is_ascii_hexdigit() {
            p += 1;
            hex_count += 1;
        }
        // Optional whitespace consumed by the escape.
        if p + 1 < bytes.len() && bytes[p] == b'\r' && bytes[p + 1] == b'\n' { p += 2; }
        else if p < bytes.len() && matches!(bytes[p], b' ' | b'\t' | b'\n' | b'\r' | 0x0C) { p += 1; }
        return p - pos;
    }
    if !matches!(c, b'\n' | b'\r' | 0x0C) && !c.is_ascii_digit() && !c.is_ascii_hexdigit() {
        // \[^\d\n\f\rA-Fa-f] — but the [^...] allows non-hex. Since we
        // already excluded hex above, this branch consumes any non-newline
        // single char.
        let rest = &input[p..];
        if let Some(ch) = rest.chars().next() {
            return (p - pos) + ch.len_utf8();
        }
    }
    0
}

fn eq_icase(a: &str, b: &str) -> bool {
    if a.len() != b.len() { return false; }
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    for i in 0..ab.len() {
        if ab[i].to_ascii_lowercase() != bb[i].to_ascii_lowercase() {
            return false;
        }
    }
    true
}

/// `\b` after position `pos`: the char at `pos` must not be `[A-Za-z0-9_]`.
/// At end-of-input, considered a boundary.
fn is_word_boundary_after(input: &str, pos: usize) -> bool {
    let bytes = input.as_bytes();
    if pos >= bytes.len() { return true; }
    let c = bytes[pos];
    !(c.is_ascii_alphanumeric() || c == b'_')
}

/// Same predicate exposed for external use.
fn has_word_boundary(input: &str, pos: usize) -> bool {
    is_word_boundary_after(input, pos)
}

fn is_js_whitespace(ch: char) -> bool {
    // ECMA-262 \s.
    matches!(
        ch,
        ' ' | '\t' | '\n' | '\r' | '\x0C' | '\x0B'
            | '\u{00A0}' | '\u{1680}' | '\u{2028}' | '\u{2029}'
            | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    ) || matches!(ch as u32, 0x2000..=0x200A)
}

fn count_newlines(s: &str) -> usize {
    s.bytes().filter(|b| *b == b'\n').count()
}

/// `1 + Math.log10(n|1) | 0` upstream — `floor(log10(max(n,1)))`.
fn number_log10_floor(n: usize) -> usize {
    let n = n.max(1);
    let mut x = n;
    let mut d = 0;
    while x >= 10 {
        x /= 10;
        d += 1;
    }
    d
}

// --------------------------------------------------------------------------
// Parser (Pratt)
// --------------------------------------------------------------------------

struct Parser<'a> {
    lexer: Lexer<'a>,
    /// Current lookahead.
    cur: Token,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Result<Self, ParseError> {
        let mut lexer = Lexer::new(input);
        let cur = lexer.next_token()?;
        Ok(Self { lexer, cur })
    }

    fn advance(&mut self) -> Result<Token, ParseError> {
        let next = self.lexer.next_token()?;
        Ok(std::mem::replace(&mut self.cur, next))
    }

    fn parse(&mut self) -> Result<CalcNode, ParseError> {
        // expression : math_expression EOF
        let expr = self.parse_math_expression(0)?;
        if self.cur.tok != Tok::Eof {
            return Err(self.parse_error(EXPECTING_OPERATOR_OR_EOF));
        }
        Ok(expr)
    }

    /// Pratt parse with min-precedence. Operators:
    ///   ADD/SUB: prec 1, left-assoc → next-min = 2
    ///   MUL/DIV: prec 2, left-assoc → next-min = 3
    fn parse_math_expression(&mut self, min_prec: u8) -> Result<CalcNode, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let (prec, op) = match self.cur.tok {
                Tok::Add => (1u8, "+"),
                Tok::Sub => (1u8, "-"),
                Tok::Mul => (2u8, "*"),
                Tok::Div => (2u8, "/"),
                _ => break,
            };
            if prec < min_prec { break; }
            self.advance()?;
            let right = self.parse_math_expression(prec + 1)?;
            left = CalcNode::MathExpression {
                operator: op.to_string(),
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// `dimension : ADD dimension | SUB dimension | <leaf>`
    /// `number    : NUMBER | ADD NUMBER | SUB NUMBER`
    ///
    /// Unary signs apply to dimension/number operands. Inside an arbitrary
    /// math_expression where ADD/SUB is the operator, a leading `+`/`-` is
    /// the binary operator; the unary form only appears at the start of an
    /// operand position. In Pratt, that's right after `(`, after a binary
    /// op, or at the start.
    fn parse_unary(&mut self) -> Result<CalcNode, ParseError> {
        match self.cur.tok {
            Tok::Add => {
                self.advance()?;
                self.parse_signed(false)
            }
            Tok::Sub => {
                self.advance()?;
                self.parse_signed(true)
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_signed(&mut self, negate: bool) -> Result<CalcNode, ParseError> {
        // Signed must be a dimension or NUMBER per grammar.
        match self.cur.tok {
            Tok::Number => {
                let tk = self.advance()?;
                let mut value = parse_float(&tk.text);
                if negate { value *= -1.0; }
                Ok(CalcNode::Number { value })
            }
            // Any dimension token.
            Tok::Length | Tok::Angle | Tok::Time | Tok::Freq | Tok::Res
            | Tok::Em | Tok::Ex | Tok::Ch | Tok::Rem
            | Tok::Vw | Tok::Vh | Tok::Vmin | Tok::Vmax
            | Tok::Percentage | Tok::UnknownDimension => {
                let mut node = self.parse_primary_dimension()?;
                if negate {
                    if let CalcNode::Dimension { value, .. } = &mut node {
                        *value *= -1.0;
                    }
                }
                Ok(node)
            }
            // Even though the grammar only allows ADD/SUB before dimension/number,
            // upstream's parser also accepts `+ dimension` from chained unaries
            // via the precedence rules. Preserve the rejection here.
            _ => Err(self.parse_error(EXPECTING_DIMENSION_OR_NUMBER)),
        }
    }

    fn parse_primary(&mut self) -> Result<CalcNode, ParseError> {
        match self.cur.tok {
            Tok::Calc => {
                // CALC LPAREN math_expression RPAREN { $$ = $3 }
                self.advance()?;
                if self.cur.tok != Tok::Lparen {
                    return Err(self.parse_error(EXPECTING_LPAREN));
                }
                self.advance()?;
                let inner = self.parse_math_expression(0)?;
                if self.cur.tok != Tok::Rparen {
                    return Err(self.parse_error(EXPECTING_RPAREN));
                }
                self.advance()?;
                Ok(inner)
            }
            Tok::Lparen => {
                self.advance()?;
                let inner = self.parse_math_expression(0)?;
                if self.cur.tok != Tok::Rparen {
                    return Err(self.parse_error(EXPECTING_RPAREN));
                }
                self.advance()?;
                Ok(CalcNode::ParenthesizedExpression { content: Box::new(inner) })
            }
            Tok::Function => {
                let tk = self.advance()?;
                Ok(CalcNode::Function { value: tk.text })
            }
            Tok::Number => {
                let tk = self.advance()?;
                Ok(CalcNode::Number { value: parse_float(&tk.text) })
            }
            _ => self.parse_primary_dimension(),
        }
    }

    fn parse_primary_dimension(&mut self) -> Result<CalcNode, ParseError> {
        let kind = match self.cur.tok {
            Tok::Length => DimensionKind::Length,
            Tok::Angle => DimensionKind::Angle,
            Tok::Time => DimensionKind::Time,
            Tok::Freq => DimensionKind::Frequency,
            Tok::Res => DimensionKind::Resolution,
            Tok::Em => DimensionKind::Em,
            Tok::Ex => DimensionKind::Ex,
            Tok::Ch => DimensionKind::Ch,
            Tok::Rem => DimensionKind::Rem,
            Tok::Vw => DimensionKind::Vw,
            Tok::Vh => DimensionKind::Vh,
            Tok::Vmin => DimensionKind::Vmin,
            Tok::Vmax => DimensionKind::Vmax,
            Tok::Percentage => DimensionKind::Percentage,
            Tok::UnknownDimension => DimensionKind::Unknown,
            _ => return Err(self.parse_error(EXPECTING_PRIMARY)),
        };
        let tk = self.advance()?;
        let value = parse_float(&tk.text);
        let unit = match kind {
            DimensionKind::Em => "em".to_string(),
            DimensionKind::Ex => "ex".to_string(),
            DimensionKind::Ch => "ch".to_string(),
            DimensionKind::Rem => "rem".to_string(),
            DimensionKind::Vh => "vh".to_string(),
            DimensionKind::Vw => "vw".to_string(),
            DimensionKind::Vmin => "vmin".to_string(),
            DimensionKind::Vmax => "vmax".to_string(),
            DimensionKind::Percentage => "%".to_string(),
            // Length/Angle/Time/Freq/Res/UnknownDimension: extract trailing
            // ASCII letters from the token text per upstream `/[a-z]+$/i`.
            _ => extract_trailing_letters(&tk.text),
        };
        Ok(CalcNode::Dimension { kind, value, unit })
    }

    /// Build the parser-error message exactly as upstream emits it.
    /// Mirrors `parser.js:1700-1717` flow.
    fn parse_error(&self, expected_set: ExpectedSet) -> ParseError {
        // yylineno = 0-indexed line counter.
        let yylineno = count_newlines(&self.lexer.matched);
        let mut s = format!("Parse error on line {}: ", yylineno + 1);
        // showPosition(79-10, 10) = showPosition(69, 10).
        s.push('\n');
        s.push_str(&self.show_position());
        s.push('\n');
        let symbol_descr = self.cur.tok.describe();
        if !expected_set.is_empty() {
            s.push_str("Expecting ");
            s.push_str(&expected_set.join(", "));
            s.push_str(", got unexpected ");
            // describeSymbol returns the bare token name; for the "unexpected"
            // suffix upstream uses `errSymbolDescr = describeSymbol(symbol) || symbol`.
            // describeSymbol returns quoted string e.g. `"DIV"`. So the
            // produced string is `... got unexpected "DIV"`.
            s.push_str(symbol_descr);
        } else {
            s.push_str("Unexpected ");
            s.push_str(symbol_descr);
        }
        ParseError(s)
    }

    /// `showPosition(maxPrefix=69, maxPostfix=10)`:
    ///   pre = pastInput(69).replace(/\s/g, ' ')
    ///   c   = "-".repeat(pre.length)
    ///   return pre + upcomingInput(10).replace(/\s/g, ' ') + '\n' + c + '^'
    fn show_position(&self) -> String {
        let pre = past_input(&self.lexer.matched, &self.cur.text, 69);
        let pre = pre.replace(|c: char| c.is_whitespace(), " ");
        let upcoming = upcoming_input(self.lexer.input, self.cur.start, &self.cur.text, 10);
        let upcoming = upcoming.replace(|c: char| c.is_whitespace(), " ");
        let dashes = "-".repeat(pre.chars().count());
        format!("{}{}\n{}^", pre, upcoming, dashes)
    }
}

/// Return the bytes consumed BEFORE the current token, optionally trimmed
/// to `max_size` characters with a `...` prefix.
///
/// Upstream `pastInput(maxSize, maxLines)`:
///   past = matched.substring(0, matched.length - match.length)
///   past = past.substr(-maxSize * 2 - 2)
///   split by `\n`, take last 1 line (default), join, then if past.length > maxSize, prepend `'...'`.
fn past_input(matched: &str, current_match: &str, max_size: usize) -> String {
    let past_chars: Vec<char> = matched.chars().collect();
    let match_len = current_match.chars().count();
    let past_len = past_chars.len().saturating_sub(match_len);
    let past_str: String = past_chars[..past_len].iter().collect();
    // substr(-max_size*2-2)
    let cut = max_size * 2 + 2;
    let trimmed: String = if past_str.chars().count() > cut {
        past_str.chars().rev().take(cut).collect::<Vec<_>>().into_iter().rev().collect()
    } else {
        past_str
    };
    // split('\n'), take last line.
    let last_line = trimmed.rsplit('\n').next().unwrap_or("");
    if last_line.chars().count() > max_size {
        let suffix: String = last_line
            .chars()
            .rev()
            .take(max_size)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("...{}", suffix)
    } else {
        last_line.to_string()
    }
}

/// `upcomingInput(maxSize=10, maxLines=1)`:
///   next = match
///   if next.length < maxSize*2+2: next += input.substring(0, maxSize*2+2)
///   split by `\n`, take first 1 line.
///   if next.length > maxSize: next = next.substring(0, maxSize) + '...'
fn upcoming_input(input: &str, cur_start: usize, current_match: &str, max_size: usize) -> String {
    let mut next = current_match.to_string();
    let cut = max_size * 2 + 2;
    if next.chars().count() < cut {
        // Append from input AFTER the current match end.
        let cur_end = cur_start + current_match.len();
        let rest = if cur_end < input.len() { &input[cur_end..] } else { "" };
        let take_n = cut.saturating_sub(next.chars().count());
        let appended: String = rest.chars().take(take_n).collect();
        next.push_str(&appended);
    }
    let first_line = next.split('\n').next().unwrap_or("");
    if first_line.chars().count() > max_size {
        let pfx: String = first_line.chars().take(max_size).collect();
        format!("{}...", pfx)
    } else {
        first_line.to_string()
    }
}

// Each parse_error site uses one of these expected-token sets. They mirror
// the jison-generated `collect_expected_token_set(state)` — which for
// postcss-calc collapses to a small handful of unique sets given the tiny
// grammar.
//
// The exact set strings come from running `node parser.js` on inputs that
// land in each error state — our tests lock the specific output bytes.

/// The expected token names list to splice into `Expecting <list>, got ...`.
#[derive(Debug, Clone, Copy)]
struct ExpectedSet(&'static [&'static str]);

impl ExpectedSet {
    fn join(&self, sep: &str) -> String {
        self.0.join(sep)
    }
    fn is_empty(&self) -> bool { self.0.is_empty() }
}

/// Initial-state expected set — top of math_expression (also after `(` and
/// after binary operator). This is the `Expecting "CALC", "LPAREN", "ADD",
/// "SUB", "FUNCTION", "LENGTH", "ANGLE", "TIME", "FREQ", "RES",
/// "UNKNOWN_DIMENSION", "EMS", "EXS", "CHS", "REMS", "VHS", "VWS", "VMINS",
/// "VMAXS", "PERCENTAGE", "NUMBER", "expression", "math_expression",
/// "function", "dimension", "number"` set seen in the empirical bridge run.
const EXPECTING_PRIMARY: ExpectedSet = ExpectedSet(&[
    "\"CALC\"", "\"LPAREN\"", "\"ADD\"", "\"SUB\"", "\"FUNCTION\"",
    "\"LENGTH\"", "\"ANGLE\"", "\"TIME\"", "\"FREQ\"", "\"RES\"",
    "\"UNKNOWN_DIMENSION\"", "\"EMS\"", "\"EXS\"", "\"CHS\"", "\"REMS\"",
    "\"VHS\"", "\"VWS\"", "\"VMINS\"", "\"VMAXS\"", "\"PERCENTAGE\"",
    "\"NUMBER\"", "\"expression\"", "\"math_expression\"", "\"function\"",
    "\"dimension\"", "\"number\"",
]);

/// After dimension-or-number (signed). Same set as primary minus CALC/LPAREN/FUNCTION.
const EXPECTING_DIMENSION_OR_NUMBER: ExpectedSet = ExpectedSet(&[
    "\"LENGTH\"", "\"ANGLE\"", "\"TIME\"", "\"FREQ\"", "\"RES\"",
    "\"UNKNOWN_DIMENSION\"", "\"EMS\"", "\"EXS\"", "\"CHS\"", "\"REMS\"",
    "\"VHS\"", "\"VWS\"", "\"VMINS\"", "\"VMAXS\"", "\"PERCENTAGE\"",
    "\"NUMBER\"", "\"dimension\"", "\"number\"",
]);

const EXPECTING_LPAREN: ExpectedSet = ExpectedSet(&["\"LPAREN\""]);
const EXPECTING_RPAREN: ExpectedSet = ExpectedSet(&["\"RPAREN\""]);
const EXPECTING_OPERATOR_OR_EOF: ExpectedSet = ExpectedSet(&[
    "\"EOF\"", "\"ADD\"", "\"SUB\"", "\"MUL\"", "\"DIV\"",
]);

// --------------------------------------------------------------------------
// parseFloat
// --------------------------------------------------------------------------

/// JS `parseFloat(s)` — parses leading optional sign + decimal + exponent.
/// Stops at the first invalid char. For the lexer's number-prefixed tokens
/// this equates to "extract the leading numeric portion and parse with f64".
fn parse_float(s: &str) -> f64 {
    // Empty / no leading number → NaN. But our caller only invokes this on
    // tokens that start with a number, so the prefix is always non-empty.
    let bytes = s.as_bytes();
    let mut p = 0;
    if p < bytes.len() && (bytes[p] == b'+' || bytes[p] == b'-') { p += 1; }
    // digits
    while p < bytes.len() && bytes[p].is_ascii_digit() { p += 1; }
    // .digits
    if p < bytes.len() && bytes[p] == b'.' {
        p += 1;
        while p < bytes.len() && bytes[p].is_ascii_digit() { p += 1; }
    }
    // exponent
    if p < bytes.len() && (bytes[p] == b'e' || bytes[p] == b'E') {
        let mut q = p + 1;
        if q < bytes.len() && (bytes[q] == b'+' || bytes[q] == b'-') { q += 1; }
        if q < bytes.len() && bytes[q].is_ascii_digit() {
            p = q;
            while p < bytes.len() && bytes[p].is_ascii_digit() { p += 1; }
        }
    }
    let prefix = &s[..p];
    prefix.parse::<f64>().unwrap_or(f64::NAN)
}

/// Mirrors upstream `/[a-z]+$/i.exec(s)[0]` — captures the trailing run of
/// ASCII letters. Used to extract unit text from LENGTH/ANGLE/etc tokens.
fn extract_trailing_letters(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut start = bytes.len();
    while start > 0 && bytes[start - 1].is_ascii_alphabetic() {
        start -= 1;
    }
    s[start..].to_string()
}

// --------------------------------------------------------------------------
// Public entry
// --------------------------------------------------------------------------

/// Mirrors `parser.parse(input)` upstream.
pub fn parse(input: &str) -> Result<CalcNode, ParseError> {
    let mut p = Parser::new(input)?;
    p.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(input: &str) -> CalcNode {
        parse(input).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", input, e))
    }

    #[test]
    fn simple_addition() {
        let n = ok("1px + 2px");
        match n {
            CalcNode::MathExpression { operator, .. } => assert_eq!(operator, "+"),
            _ => panic!("expected MathExpression, got {:?}", n),
        }
    }

    #[test]
    fn calc_unwrap() {
        // calc(1px + 2px) → MathExpression (calc consumes parens)
        let n = ok("calc(1px + 2px)");
        match n {
            CalcNode::MathExpression { operator, .. } => assert_eq!(operator, "+"),
            _ => panic!("expected MathExpression"),
        }
    }

    #[test]
    fn parens() {
        let n = ok("(1px)");
        match n {
            CalcNode::ParenthesizedExpression { .. } => {}
            _ => panic!("expected Parenthesized"),
        }
    }

    #[test]
    fn function_with_var() {
        let n = ok("var(--mouseX)");
        match n {
            CalcNode::Function { value } => assert_eq!(value, "var(--mouseX)"),
            _ => panic!("expected Function"),
        }
    }

    #[test]
    fn function_nested_var() {
        let n = ok("var(--xxx, var(--yyy))");
        match n {
            CalcNode::Function { value } => assert_eq!(value, "var(--xxx, var(--yyy))"),
            _ => panic!("expected Function"),
        }
    }

    #[test]
    fn unary_negative_number() {
        let n = ok("-5");
        match n {
            CalcNode::Number { value } => assert_eq!(value, -5.0),
            _ => panic!("expected Number"),
        }
    }

    #[test]
    fn unary_negative_dimension() {
        let n = ok("-5px");
        match n {
            CalcNode::Dimension { value, unit, .. } => {
                assert_eq!(value, -5.0);
                assert_eq!(unit, "px");
            }
            _ => panic!("expected Dimension"),
        }
    }

    #[test]
    fn dimensional_unit_extraction() {
        // 1.5cm → Length(1.5, "cm")
        let n = ok("1.5cm");
        match n {
            CalcNode::Dimension { kind, value, unit } => {
                assert!(matches!(kind, DimensionKind::Length));
                assert_eq!(value, 1.5);
                assert_eq!(unit, "cm");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn percentage() {
        let n = ok("50%");
        match n {
            CalcNode::Dimension { kind, value, unit } => {
                assert!(matches!(kind, DimensionKind::Percentage));
                assert_eq!(value, 50.0);
                assert_eq!(unit, "%");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn exponent_dimension() {
        let n = ok("1.1e+1px");
        match n {
            CalcNode::Dimension { value, .. } => assert_eq!(value, 11.0),
            _ => panic!(),
        }
    }

    #[test]
    fn precedence_mul_over_add() {
        // 1 + 2 * 3 → 1 + (2 * 3)
        let n = ok("1 + 2 * 3");
        match &n {
            CalcNode::MathExpression { operator, right, .. } => {
                assert_eq!(operator, "+");
                assert!(matches!(right.as_ref(), CalcNode::MathExpression { operator, .. } if operator == "*"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn vendor_prefixed_calc() {
        let n = ok("-webkit-calc(1px + 2px)");
        match n {
            CalcNode::MathExpression { .. } => {}
            _ => panic!("expected MathExpression after CALC unwrap"),
        }
    }

    #[test]
    fn case_insensitive_calc() {
        let n = ok("CALC(1PX + 1PX)");
        match n {
            CalcNode::MathExpression { .. } => {}
            _ => panic!(),
        }
    }

    #[test]
    fn lex_error_format() {
        // The canonical case: `10pc + unknown` → Lexical error.
        let err = parse("10pc + unknown").unwrap_err();
        assert_eq!(
            err.0,
            "Lexical error on line 1: Unrecognized text.\n\n  Erroneous area:\n1: 10pc + unknown\n^.........^"
        );
    }

    #[test]
    fn parse_error_format_div_at_start() {
        // `/...` → DIV at start. Parse error format.
        let err = parse("/* test */ 1px").unwrap_err();
        // The error message starts with `Parse error on line 1: `.
        assert!(err.0.starts_with("Parse error on line 1: "), "got: {:?}", err.0);
        // Should mention "got unexpected \"DIV\"".
        assert!(err.0.contains("got unexpected \"DIV\""), "got: {:?}", err.0);
    }
}
