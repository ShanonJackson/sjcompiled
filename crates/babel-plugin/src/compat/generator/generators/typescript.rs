//! 1:1 port of the slice of `@babel/generator@7.23.0/lib/generators/typescript.js`
//! reachable from Compiled's `generate(expression)` call sites.
//!
//! Why this slice and not the whole file:
//! Upstream's `typescript.js` is 694 LOC covering every TS AST node,
//! most of which are statement-level (TSEnumDeclaration,
//! TSTypeAliasDeclaration, TSInterfaceDeclaration, etc) and never
//! reachable from `generate(&Expr)`. Compiled's evaluator only feeds
//! Expression-level nodes into `generate()`, so the only surface we
//! need is:
//!  - TS expression wrappers — `TSAsExpression`, `TSSatisfiesExpression`,
//!    `TSTypeAssertion`, `TSNonNullExpression`, `TSConstAssertion`,
//!    `TSInstantiationExpression` — these wrap an inner Expression
//!    and re-enter the printer.
//!  - The TS Type tree they reference — `TSKeywordType` (number,
//!    string, boolean, …), `TSTypeReference` (named type), the
//!    qualified-name path inside, and a handful of literal types.
//!
//! Reachability evidence: `fixtures/ct-ts-as-cast/input.tsx` exercises
//! `(props.lineclamp as number) * 1.42857142857143` — a `TsAsExpr`
//! with a `TsKeywordType(Number)` annotation, generated as input to
//! the CSS-variable hash. The `/*UNHANDLED-EXPR*/` placeholder
//! that this module replaces was directly hashed before this port,
//! producing a diverged class name.
//!
//! Per CLAUDE.md compat policy: when a future fixture surfaces a
//! TS node kind not in the slice below (e.g. `as { foo: T }`
//! with TSTypeLiteral), port that node and its transitive type
//! dependencies 1:1 from upstream — DO NOT improvise. The hash
//! contract requires byte-equality with `@babel/generator`.

use crate::compat::generator::printer::Printer;

use swc_core::ecma::ast::{
    Expr, Ident, TsAsExpr, TsConstAssertion, TsEntityName, TsInstantiation, TsKeywordType,
    TsKeywordTypeKind, TsNonNullExpr, TsQualifiedName, TsSatisfiesExpr, TsType, TsTypeAssertion,
    TsTypeRef,
};

/// `TSAsExpression` — `expression as typeAnnotation`.
///
/// Upstream: `typescript.js:498-510`.
pub fn ts_as_expr(p: &mut Printer, node: &TsAsExpr, parent: &Expr) {
    p.print(&node.expr, Some(parent));
    p.space();
    p.word("as");
    p.space();
    ts_type_inner(p, &node.type_ann, parent);
}

/// `TSSatisfiesExpression` — `expression satisfies typeAnnotation`.
pub fn ts_satisfies_expr(p: &mut Printer, node: &TsSatisfiesExpr, parent: &Expr) {
    p.print(&node.expr, Some(parent));
    p.space();
    p.word("satisfies");
    p.space();
    ts_type_inner(p, &node.type_ann, parent);
}

/// `TSTypeAssertion` — `<typeAnnotation>expression`.
///
/// Upstream: `typescript.js:511-519`. Note the SPACE between `>` and
/// the expression (Babel emits `<T> expr`, not `<T>expr`).
pub fn ts_type_assertion(p: &mut Printer, node: &TsTypeAssertion, parent: &Expr) {
    p.token_char(b'<');
    ts_type_inner(p, &node.type_ann, parent);
    p.token_char(b'>');
    p.space();
    p.print(&node.expr, Some(parent));
}

/// `TSNonNullExpression` — `expression!`.
///
/// Upstream: `typescript.js:633-636`.
pub fn ts_non_null_expr(p: &mut Printer, node: &TsNonNullExpr, parent: &Expr) {
    p.print(&node.expr, Some(parent));
    p.token_char(b'!');
}

/// `as const` — appears as a dedicated `TsConstAssertion` node in
/// SWC; in Babel this is a `TSAsExpression` whose typeAnnotation is
/// `TSTypeReference { typeName: Identifier("const") }`. Both produce
/// the same emitted bytes: `expression as const`.
pub fn ts_const_assertion(p: &mut Printer, node: &TsConstAssertion, parent: &Expr) {
    p.print(&node.expr, Some(parent));
    p.space();
    p.word("as");
    p.space();
    p.word("const");
}

/// `TSInstantiationExpression` — `expression<typeArgs>`.
///
/// Upstream: `typescript.js:520-523`. Type-arg printing is omitted
/// from this slice (no fixture currently reaches non-empty type
/// arguments through the hash path); a future fixture that does
/// MUST trigger a port of `tsPrintTypeParameters`, not a workaround.
pub fn ts_instantiation(p: &mut Printer, node: &TsInstantiation, parent: &Expr) {
    p.print(&node.expr, Some(parent));
    // Type params left unimplemented — see module note. If a real
    // fixture hits this, the placeholder makes the divergence visible
    // rather than silently dropping bytes.
    if node.type_args.params.iter().any(|_| true) {
        p.buf.append("/*UNHANDLED-TYPE-ARGS*/");
    }
}

// ---------------------------------------------------------------------------
// TS type tree — only the variants reachable from the call-site corpus.
// ---------------------------------------------------------------------------

/// Walk a TsType and print it verbatim per upstream's branches.
///
/// Each match arm cites the upstream `typescript.js` function it
/// mirrors so a future TS-node addition tracks back to the same line.
pub fn ts_type_inner(p: &mut Printer, ty: &TsType, parent: &Expr) {
    match ty {
        TsType::TsKeywordType(k) => ts_keyword_type(p, k),
        TsType::TsTypeRef(r) => ts_type_ref(p, r, parent),
        TsType::TsThisType(_) => p.word("this"),
        // Anything else — emit a clearly-marked placeholder so the
        // divergence surfaces in fixtures-triage. Per CLAUDE.md no
        // silent drops; no improvisation either — the next fixture
        // that hits this branch should trigger a 1:1 port of the
        // matching upstream handler.
        _ => {
            p.buf.append("/*UNHANDLED-TS-TYPE*/");
        }
    }
}

/// `TSKeywordType` — `any|bigint|unknown|number|object|boolean|string|symbol|void|undefined|null|never|intrinsic`.
///
/// Upstream: `typescript.js:211-249` (one one-line function per kind).
fn ts_keyword_type(p: &mut Printer, k: &TsKeywordType) {
    let word = match k.kind {
        TsKeywordTypeKind::TsAnyKeyword => "any",
        TsKeywordTypeKind::TsBigIntKeyword => "bigint",
        TsKeywordTypeKind::TsUnknownKeyword => "unknown",
        TsKeywordTypeKind::TsNumberKeyword => "number",
        TsKeywordTypeKind::TsObjectKeyword => "object",
        TsKeywordTypeKind::TsBooleanKeyword => "boolean",
        TsKeywordTypeKind::TsStringKeyword => "string",
        TsKeywordTypeKind::TsSymbolKeyword => "symbol",
        TsKeywordTypeKind::TsVoidKeyword => "void",
        TsKeywordTypeKind::TsUndefinedKeyword => "undefined",
        TsKeywordTypeKind::TsNullKeyword => "null",
        TsKeywordTypeKind::TsNeverKeyword => "never",
        TsKeywordTypeKind::TsIntrinsicKeyword => "intrinsic",
    };
    p.word(word);
}

/// `TSTypeReference` — `typeName<typeParameters>`.
///
/// Upstream: `typescript.js:280-283`. Type-parameter printing is
/// omitted (same rationale as `ts_instantiation`); the fixture
/// corpus reaches only bare references like `MyType` or
/// qualified names like `Foo.Bar`.
fn ts_type_ref(p: &mut Printer, n: &TsTypeRef, parent: &Expr) {
    ts_entity_name(p, &n.type_name, parent);
    if n.type_params.is_some() {
        p.buf.append("/*UNHANDLED-TYPE-PARAMS*/");
    }
}

fn ts_entity_name(p: &mut Printer, name: &TsEntityName, parent: &Expr) {
    match name {
        TsEntityName::Ident(i) => {
            // Reuse the Ident printer via the unified `Expr::Ident`
            // path so any ident-side nuance (escape rules etc) stays
            // 1:1 with non-TS sites.
            let e = Expr::Ident(i.clone());
            p.print(&e, Some(parent));
        }
        TsEntityName::TsQualifiedName(q) => ts_qualified_name(p, q, parent),
    }
}

/// `TSQualifiedName` — `left.right`.
///
/// Upstream: `typescript.js:134-138`.
#[cfg(test)]
mod tests {
    use crate::compat::generator::generate;
    use swc_core::common::{sync::Lrc, FileName, SourceMap};
    use swc_core::ecma::ast::{EsVersion, Expr, Stmt};
    use swc_core::ecma::parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

    fn parse_first_expr(src: &str) -> Expr {
        let cm: Lrc<SourceMap> = Default::default();
        let fm = cm.new_source_file(FileName::Anon.into(), src.to_string());
        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            EsVersion::Es2022,
            StringInput::from(&*fm),
            None,
        );
        let mut p = Parser::new_from(lexer);
        let module = p.parse_module().expect("parse");
        let stmt = module.body.into_iter().next().expect("first stmt");
        match stmt {
            swc_core::ecma::ast::ModuleItem::Stmt(Stmt::Expr(es)) => *es.expr,
            other => panic!("expected ExprStmt, got {:?}", other),
        }
    }

    #[test]
    fn ts_as_in_binary_matches_babel() {
        let expr = parse_first_expr("(props.lineclamp as number) * 1.42857142857143;");
        // Babel @babel/generator output is locked at:
        //  "(props.lineclamp as number) * 1.42857142857143"
        // (verified via parity-harness/probe-ts-as.mjs).
        let got = generate(&expr);
        assert_eq!(got, "(props.lineclamp as number) * 1.42857142857143");
    }

    #[test]
    fn arrow_with_regex_literal_replace() {
        // Mirrors fixtures/ct-chart-legend-content-regex — Babel
        // @babel/generator output (verified via probe-regex-arrow.mjs):
        //   "__cmplp => JSON.stringify((__cmplp.name || '').replace(/\\/g, ''))"
        let module_src = r"__cmplp => JSON.stringify((__cmplp.name || '').replace(/\\/g, ''));";
        let expr = parse_first_expr(module_src);
        let got = generate(&expr);
        assert_eq!(
            got,
            r"__cmplp => JSON.stringify((__cmplp.name || '').replace(/\\/g, ''))"
        );
    }

    #[test]
    fn arrow_with_ts_param_and_inner_as_cast() {
        // Mirrors fixtures/ct-ts-as-cast: ArrowFunctionExpression
        // whose param has a TS type annotation `: Props` and whose
        // body is a TemplateLiteral with an inner `as number` cast.
        // Babel @babel/generator output (locked via
        // parity-harness/probe-arrow-ts.mjs):
        //   "(props: Props) => `${(props.lineclamp as number) * 1.42857142857143}em`"
        let module_src = "(props: Props) => `${(props.lineclamp as number) * 1.42857142857143}em`;";
        let expr = parse_first_expr(module_src);
        let got = generate(&expr);
        assert_eq!(
            got,
            "(props: Props) => `${(props.lineclamp as number) * 1.42857142857143}em`"
        );
    }

    #[test]
    fn ts_as_no_user_parens_still_emits_parens() {
        // Without source-side parens, Babel still wraps because of
        // `parentheses.js:165 — TSAsExpression returns true`. SWC's
        // parser may strip the parens at parse time (Paren node not
        // emitted around the cast) — verify the unconditional rule fires.
        let expr = parse_first_expr("props.x as number * 2;");
        let got = generate(&expr);
        // Babel: `(props.x as number) * 2` regardless of source parens.
        assert_eq!(got, "(props.x as number) * 2");
    }

    #[test]
    fn ts_non_null_matches_babel() {
        let expr = parse_first_expr("foo!.bar;");
        let got = generate(&expr);
        assert_eq!(got, "foo!.bar");
    }

    // `as const` parity is deferred — SWC parses it as TsConstAssertion
    // (a dedicated AST node) while Babel parses it as TSAsExpression
    // with `typeAnnotation: TSTypeReference("const")`. Adding a parity
    // test requires probing Babel's exact output for the Compiled
    // call sites that reach `as const` (none in the current fixture
    // corpus); skip for now and add when a real fixture surfaces it.
}

fn ts_qualified_name(p: &mut Printer, n: &TsQualifiedName, parent: &Expr) {
    ts_entity_name(p, &n.left, parent);
    p.token_char(b'.');
    // SWC's `TsQualifiedName.right` is `IdentName` (no syntax context);
    // wrap it as a regular Ident so the unified Expr::Ident printer
    // path handles escape rules identically to the lhs entity name.
    let id = Ident::new(n.right.sym.clone(), n.right.span, Default::default());
    let e = Expr::Ident(id);
    p.print(&e, Some(parent));
}
