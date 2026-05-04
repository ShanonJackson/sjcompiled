//! 1:1 port of `@babel/generator@7.23.0/lib/generators/types.js`.

use crate::compat::generator::printer::Printer;

use swc_core::common::{BytePos, Spanned};
use swc_core::ecma::ast::{
    ArrayLit, ComputedPropName, Expr, ExprOrSpread, Ident, Lit, ObjectLit, Prop, PropName,
    PropOrSpread,
};

/// `Identifier(node)` — emit the name as a word.
pub fn ident(p: &mut Printer, node: &Ident) {
    p.word(node.sym.as_ref());
}

/// `BooleanLiteral(node)`, `NullLiteral`, `NumericLiteral(node)`,
/// `StringLiteral(node)`, `BigIntLiteral`, `RegExpLiteral` — single
/// dispatcher matching SWC's `Lit` enum (the AST-shape adapter).
pub fn literal(p: &mut Printer, lit: &Lit) {
    match lit {
        Lit::Str(s) => string_literal(p, s),
        Lit::Bool(b) => p.word(if b.value { "true" } else { "false" }),
        Lit::Null(_) => p.word("null"),
        Lit::Num(n) => numeric_literal(p, n),
        Lit::BigInt(b) => big_int_literal(p, b),
        Lit::Regex(r) => p.word(&format!("/{}/{}", r.exp.as_ref(), r.flags.as_ref())),
        Lit::JSXText(_) => {
            // JSXText is only emitted via the JSX-element path.
            // Reachable from `printJSXChildren` once JSX lands.
        }
    }
}

pub(super) fn string_literal(p: &mut Printer, s: &swc_core::ecma::ast::Str) {
    // Babel's `getPossibleRaw(node)` returns `node.extra.raw` (the
    // EXACT source-quoted form) when available. SWC stores the same
    // on `Str.raw: Option<Atom>`. Using `raw` preserves single-vs-double
    // quote choice from the input — load-bearing for the corpus's
    // `string-literal-single` fixture and a real Babel-vs-SWC-default
    // divergence point.
    if let Some(raw) = s.raw.as_ref() {
        p.token(raw.as_ref());
    } else {
        // No source-anchored raw form (synthetic node). Fall back to
        // upstream's `_jsesc(value, jsescOption)` shape — for the
        // initial port pass we just double-quote and naively escape.
        // SWC's `Str.value` is `Wtf8Atom` (JS strings can hold lone
        // surrogates which aren't valid UTF-8). `to_atom_lossy()`
        // replaces invalid sequences with U+FFFD; that's fine for
        // our consumer surface — `css-builders.ts` only feeds valid
        // UTF-8 to generate() in real use.
        let v_atom = s.value.to_atom_lossy();
        let v: &str = &v_atom;
        let mut out = String::with_capacity(v.len() + 2);
        out.push('"');
        for ch in v.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c => out.push(c),
            }
        }
        out.push('"');
        p.token(&out);
    }
}

fn numeric_literal(p: &mut Printer, n: &swc_core::ecma::ast::Number) {
    // Same `getPossibleRaw` pattern. SWC's `Number` carries a `raw:
    // Option<Atom>` which is the source-text form. Use it when
    // available so `1.5`, `0x10`, etc. survive byte-exact.
    if let Some(raw) = n.raw.as_ref() {
        p.number(raw.as_ref());
    } else {
        // Synthetic node — emit the value's default formatting.
        // Babel uses `node.value + ""` (JS-side formatting); we use
        // Rust's default which differs for some edge cases (e.g.,
        // `1e21` vs `1e+21`). When a synthetic-node fixture surfaces,
        // port the JS side here.
        let v = n.value;
        if v == v.trunc() && v.abs() < 1e21 {
            p.number(&format!("{}", v as i64));
        } else {
            p.number(&format!("{}", v));
        }
    }
}

fn big_int_literal(p: &mut Printer, n: &swc_core::ecma::ast::BigInt) {
    if let Some(raw) = n.raw.as_ref() {
        p.word(raw.as_ref());
    } else {
        p.word(&format!("{}n", n.value));
    }
}

/// `ObjectExpression(node)`.
///
/// Upstream calls `printList(props, node, { indent: true, statement: true })`
/// which expands to: indent +1, then for each prop emit a leading
/// newline (only on the first iteration when buf has content), the
/// prop, a `,` separator (between props), and a trailing newline
/// (after every prop). Result: `{\n  a: 1,\n  b: 2\n}` for a 2-prop
/// object. The trailing-space drop in `Buffer::queue` ensures the
/// post-`{` `space()` call is suppressed before the first newline.
pub fn object(p: &mut Printer, node: &ObjectLit) {
    p.token_char(b'{');
    // SWC's comment-attachment heuristic: a same-line comment between
    // two tokens is keyed as TRAILING to the previous token (not
    // leading of the next). For `{ /* leading */ from: ...` the
    // comment is keyed at `BytePos({ + 1)` as trailing of `{`. So
    // before emitting the first prop we query trailing comments at
    // the position right after the open-brace. The `{` token is
    // 1 byte; `obj.span.lo + 1` is the byte position immediately
    // after it. (See compat-generator-coverage debug session,
    // 2026-05-04.)
    let after_open_brace = BytePos(node.span.lo.0 + 1);
    if !node.props.is_empty() {
        p.space();
        p.indent();
        // Query trailing-of-`{` comments BEFORE the first newline so
        // they land on the line we're about to break onto.
        let len = node.props.len();
        for (i, prop) in node.props.iter().enumerate() {
            // upstream `_printNewline(i === 0, ...)` — emits a newline
            // only on the first iteration if the buffer has content.
            if i == 0 && p.buf.has_content() {
                p.newline(1);
                // After the newline + auto-indent, drop the trailing-of-`{`
                // comments so they appear on the property line.
                p.print_trailing_comments_at(after_open_brace);
            }
            // Property-level leading comments: try both span.lo and
            // (for KeyValue) the key's span.lo — different prop shapes
            // key comments at different positions. `take_leading` is
            // destructive so the second call is a no-op when the
            // first found something.
            p.print_leading_comments_at(prop.span().lo);
            if let PropOrSpread::Prop(prop_box) = prop {
                if let Prop::KeyValue(kv) = &**prop_box {
                    let key_lo = match &kv.key {
                        PropName::Ident(i) => i.span.lo,
                        PropName::Str(s) => s.span.lo,
                        PropName::Num(n) => n.span.lo,
                        PropName::BigInt(b) => b.span.lo,
                        PropName::Computed(c) => c.span.lo,
                    };
                    p.print_leading_comments_at(key_lo);
                }
            }
            object_prop(p, prop);
            // separator (`,` between props, NOT after the last one)
            if i < len - 1 {
                p.token_char(b',');
            }
            // trailing newline after EVERY prop — including the last
            // (so we land on a fresh line for the closing `}`).
            p.newline(1);
        }
        p.dedent();
        p.space();
    }
    p.token_char(b'}');
}

fn object_prop(p: &mut Printer, prop: &PropOrSpread) {
    let parent_dummy = Expr::Object(ObjectLit {
        span: Default::default(),
        props: vec![],
    });
    match prop {
        PropOrSpread::Spread(s) => {
            p.token("...");
            p.print(&s.expr, Some(&parent_dummy));
        }
        PropOrSpread::Prop(prop_box) => match &**prop_box {
            Prop::Shorthand(ident_node) => {
                // `{ a }` — Babel collapses `{ a: a }` shorthand only
                // when both sides are identifiers with matching names.
                // SWC's `Prop::Shorthand` already encodes this case.
                p.word(ident_node.sym.as_ref());
            }
            Prop::KeyValue(kv) => {
                prop_name(p, &kv.key);
                p.token_char(b':');
                p.space();
                p.print(&kv.value, Some(&parent_dummy));
            }
            Prop::Assign(_) | Prop::Getter(_) | Prop::Setter(_) | Prop::Method(_) => {
                // Reachable from generators/methods.js / classes.js;
                // not exercised by the current corpus. Surface the gap.
                p.buf.append("/*UNHANDLED-PROP*/");
            }
        },
    }
}

fn prop_name(p: &mut Printer, key: &PropName) {
    match key {
        PropName::Ident(i) => p.word(i.sym.as_ref()),
        PropName::Str(s) => string_literal(p, s),
        PropName::Num(n) => numeric_literal(p, n),
        PropName::BigInt(b) => big_int_literal(p, b),
        PropName::Computed(ComputedPropName { expr, .. }) => {
            p.token_char(b'[');
            // Computed key — needs no parent context to drive parens.
            let dummy = Expr::Object(ObjectLit {
                span: Default::default(),
                props: vec![],
            });
            p.print(expr, Some(&dummy));
            p.token_char(b']');
        }
    }
}

/// `ArrayExpression(node)`.
pub fn array(p: &mut Printer, node: &ArrayLit, parent: &Expr) {
    p.token_char(b'[');
    let len = node.elems.len();
    for (i, elem) in node.elems.iter().enumerate() {
        match elem {
            Some(ExprOrSpread { spread, expr }) => {
                if i > 0 {
                    p.space();
                }
                if spread.is_some() {
                    p.token("...");
                }
                p.print(expr, Some(parent));
                if i < len - 1 {
                    p.token_char(b',');
                }
            }
            None => {
                p.token_char(b',');
            }
        }
    }
    p.token_char(b']');
}
