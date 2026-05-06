//! Babel-parser parity: normalise template literal `raw` values so the
//! AST matches what `@babel/parser@7.29.2` produces.
//!
//! Why this exists
//! ---------------
//! ECMAScript 2024 §12.8.6 ("Template Literal Lexical Components")
//! defines TRV (Template Raw Value) such that any
//! `<CR>` or `<CR><LF>` sequence in the raw source resolves to a
//! single `<LF>` (U+000A) code unit. `@babel/parser` implements this
//! verbatim (`tokenizer/index.ts`, `readTmplToken` branch on
//! `charCodes.carriageReturn`). `swc_ecma_parser` does NOT — it
//! preserves the source bytes literally, so on a CRLF source file
//! `TplElement.raw` carries `\r\n` where Babel would carry `\n`.
//!
//! Observable consequence: `compat::generator::generate` walks the
//! SWC AST and writes `quasi.raw` verbatim. The Compiled keyframes
//! site at `packages/babel-plugin/src/utils/css-builders.ts:464`
//! computes `name = k${hash(generate(expression).code)}`, so a CR-or-LF
//! difference in raw bytes flips the class-name hash for every
//! keyframes-using fixture on a CRLF checkout. Same hazard applies to
//! every other site that reads `quasi.raw` — `manipulate_template_literal`,
//! `object_property_to_string`, etc.
//!
//! Where the fix lives
//! -------------------
//! Single one-shot pre-pass over the SWC `Program` immediately after
//! parse, before any plugin visitor runs. Mutating `raw` in place
//! makes every downstream consumer see Babel-shape bytes for free —
//! no per-site fixups required, and no risk of forgetting a site
//! when adding a new one.
//!
//! Per CLAUDE.md drift policy: this is not a behavioural fix to
//! upstream Babel — Babel's parser already normalises. We're aligning
//! the SWC AST to the Babel AST so the rest of the port can stay
//! 1:1. Filed under `compat/*` per the same precedent as
//! `compat/generator`, `compat/jsesc`, etc.

use swc_core::ecma::ast::{Program, TplElement};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

/// Normalise CR / CRLF → LF inside every `TplElement.raw` in the program.
///
/// Mutates the program in place. The `cooked` value is already LF on
/// the SWC side (SWC's tokenizer DOES normalise the cooked value), so
/// this pass touches `raw` only.
pub fn normalize_template_literal_raw(program: &mut Program) {
    let mut v = TemplateRawNormalizer;
    program.visit_mut_with(&mut v);
}

struct TemplateRawNormalizer;

impl VisitMut for TemplateRawNormalizer {
    fn visit_mut_tpl_element(&mut self, n: &mut TplElement) {
        // Hot path: ASCII-only no-CR raw is the common case. Avoid
        // allocating in that case.
        if !n.raw.as_bytes().contains(&b'\r') {
            return;
        }
        let raw = n.raw.as_str();
        let mut out = String::with_capacity(raw.len());
        let bytes = raw.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'\r' {
                // CR or CRLF → single LF.
                out.push('\n');
                if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    i += 2;
                } else {
                    i += 1;
                }
            } else {
                // Push raw byte; safe because we never split a UTF-8
                // sequence (CR is single-byte, only ASCII path mutates).
                out.push(b as char);
                i += 1;
            }
        }
        // SWC's TplElement.raw is `Atom` — we go via `String` → `Atom`.
        n.raw = out.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::DUMMY_SP;
    use swc_core::ecma::ast::{Module, TplElement};

    fn make_tpl(raw: &str) -> TplElement {
        TplElement {
            span: DUMMY_SP,
            tail: true,
            cooked: None,
            raw: raw.into(),
        }
    }

    #[test]
    fn crlf_collapses_to_lf() {
        let mut t = make_tpl("\r\n  from { opacity: 0; }\r\n  to { opacity: 1; }\r\n");
        let mut v = TemplateRawNormalizer;
        v.visit_mut_tpl_element(&mut t);
        assert_eq!(&*t.raw, "\n  from { opacity: 0; }\n  to { opacity: 1; }\n");
    }

    #[test]
    fn bare_cr_collapses_to_lf() {
        let mut t = make_tpl("a\rb\rc");
        let mut v = TemplateRawNormalizer;
        v.visit_mut_tpl_element(&mut t);
        assert_eq!(&*t.raw, "a\nb\nc");
    }

    #[test]
    fn lf_only_is_left_alone() {
        let raw = "a\nb\nc";
        let mut t = make_tpl(raw);
        let mut v = TemplateRawNormalizer;
        v.visit_mut_tpl_element(&mut t);
        assert_eq!(&*t.raw, raw);
    }

    #[test]
    fn no_line_terminators_is_left_alone() {
        let raw = "color: red;";
        let mut t = make_tpl(raw);
        let mut v = TemplateRawNormalizer;
        v.visit_mut_tpl_element(&mut t);
        assert_eq!(&*t.raw, raw);
    }

    #[test]
    fn whole_program_walk_visits_nested() {
        // Build a Module containing a TaggedTpl whose tpl has a CRLF
        // raw — confirm the walker reaches into it.
        use swc_core::ecma::ast::{Expr, Ident, ModuleItem, Stmt, ExprStmt, TaggedTpl, Tpl};
        let inner = make_tpl("a\r\nb");
        let tpl = Tpl {
            span: DUMMY_SP,
            exprs: vec![],
            quasis: vec![inner],
        };
        let tagged = TaggedTpl {
            span: DUMMY_SP,
            ctxt: Default::default(),
            tag: Box::new(Expr::Ident(Ident::new("k".into(), DUMMY_SP, Default::default()))),
            type_params: None,
            tpl: Box::new(tpl),
        };
        let mut module = Module {
            span: DUMMY_SP,
            shebang: None,
            body: vec![ModuleItem::Stmt(Stmt::Expr(ExprStmt {
                span: DUMMY_SP,
                expr: Box::new(Expr::TaggedTpl(tagged)),
            }))],
        };
        let mut prog = Program::Module(module.clone());
        normalize_template_literal_raw(&mut prog);
        // Pull the module back out and assert.
        if let Program::Module(m) = prog {
            module = m;
        }
        let stmt = module.body.into_iter().next().unwrap();
        let ModuleItem::Stmt(Stmt::Expr(es)) = stmt else { panic!() };
        let Expr::TaggedTpl(t) = *es.expr else { panic!() };
        assert_eq!(&*t.tpl.quasis[0].raw, "a\nb");
    }
}
