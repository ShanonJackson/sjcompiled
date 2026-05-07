//! 1:1 port of `packages/babel-plugin/src/utils/traversers/object.ts`.
//!
//! Single function: walk an `ObjectExpression` for an
//! `ObjectProperty` whose key matches a name; return the value
//! expression. Upstream uses `traverse(object, { noScope: true,
//! ObjectProperty(path) { ... } })` with an early `path.stop()` —
//! the Rust port walks `props` directly because object expressions
//! don't nest scope; the JS `noScope: true` is a tell.

use swc_core::ecma::ast::{ObjectLit, Prop, PropName, PropOrSpread};
// `Expr` is used by the test mod's helper. Pre-import keeps the
// test mod's `use super::*;` shape lean.
#[cfg(test)]
use swc_core::ecma::ast::Expr;

use super::types::ExportResult;

/// Find the value of an object-property whose key matches
/// `property_name`. Returns `None` when:
///
/// - The property doesn't exist in the object.
/// - The property is a method / getter / setter / spread (these
///   don't have a foldable value expression — upstream's
///   `ObjectProperty` predicate excludes them).
/// - The property's key isn't an identifier OR a string literal
///   matching `property_name`. Computed keys (`[expr]: ...`) deopt
///   per upstream's `t.isIdentifier(path.node.key, { name })`
///   check — the JS plugin only matches identifier-shaped keys.
pub fn get_object_property_value(
    object: &ObjectLit,
    property_name: &str,
) -> Option<ExportResult> {
    for prop in &object.props {
        let PropOrSpread::Prop(boxed) = prop else {
            continue;
        };
        // §6.8n — normalise Prop::Shorthand into a synthetic
        // KeyValue. Babel parses `{ color }` as ObjectProperty
        // `{ key: Ident("color"), value: Ident("color"), shorthand: true }`
        // so upstream's `t.isObjectProperty(prop)` predicate matches
        // shorthand AND longhand identically. SWC splits the same
        // source into `Prop::Shorthand(Ident)` vs `Prop::KeyValue` —
        // dropping shorthand here was a pre-existing port miss
        // (same root cause as §6.8m's `extract_object_expression`
        // fix). Method / Setter / Getter / Assign are NOT
        // ObjectProperty in Babel and are correctly skipped.
        let key_sym: &swc_core::ecma::atoms::Atom;
        let value: Box<swc_core::ecma::ast::Expr>;
        match &**boxed {
            Prop::KeyValue(kv) => {
                let PropName::Ident(id) = &kv.key else {
                    // Upstream's `t.isIdentifier(path.node.key, { name })`
                    // check is FALSE for string-literal keys, so we
                    // mirror that by NOT matching here.
                    continue;
                };
                key_sym = &id.sym;
                value = kv.value.clone();
            }
            Prop::Shorthand(id) => {
                key_sym = &id.sym;
                value = Box::new(swc_core::ecma::ast::Expr::Ident(id.clone()));
            }
            _ => continue,
        }
        if *key_sym == *property_name {
            return Some(ExportResult { node: Some(value), reexport_from: None });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::{EsVersion, ExprStmt, ModuleItem, Stmt};
    use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

    fn parse_object_expression(src: &str) -> ObjectLit {
        let cm: Lrc<SourceMap> = Lrc::new(SourceMap::default());
        let fm = cm.new_source_file(Lrc::new(FileName::Anon), src.to_string());
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax {
                tsx: false,
                ..Default::default()
            }),
            EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .unwrap_or_else(|e| panic!("parse failure: {e:?}"));
        // Source is wrapped as `(<obj>);` so the parser surfaces it as
        // an ExpressionStatement with a Paren-wrapped ObjectLit.
        let ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) = &module.body[0] else {
            panic!("expected expression statement");
        };
        let inner = match &**expr {
            Expr::Paren(p) => &p.expr,
            other => panic!("expected paren expr, got {other:?}"),
        };
        let Expr::Object(obj) = &**inner else {
            panic!("expected object expr");
        };
        obj.clone()
    }

    fn expect_string_literal(node: &Option<Box<Expr>>) -> String {
        match node.as_deref() {
            Some(Expr::Lit(swc_core::ecma::ast::Lit::Str(s))) => {
                s.value.as_str().unwrap_or_default().to_string()
            }
            other => panic!("expected string literal, got {other:?}"),
        }
    }

    #[test]
    fn finds_property_by_identifier_key() {
        let obj = parse_object_expression(r#"({ red: 'r', blue: 'b' });"#);
        let r = get_object_property_value(&obj, "blue").expect("found");
        assert_eq!(expect_string_literal(&r.node), "b");
    }

    #[test]
    fn missing_property_returns_none() {
        let obj = parse_object_expression(r#"({ red: 'r' });"#);
        assert!(get_object_property_value(&obj, "blue").is_none());
    }

    #[test]
    fn string_key_does_not_match() {
        // Upstream's isIdentifier check is false for string-literal keys.
        let obj = parse_object_expression(r#"({ "blue": 'b' });"#);
        assert!(
            get_object_property_value(&obj, "blue").is_none(),
            "string-literal key must NOT match — preserves upstream behaviour"
        );
    }

    #[test]
    fn computed_key_does_not_match() {
        let obj = parse_object_expression(r#"({ ['blue']: 'b' });"#);
        assert!(get_object_property_value(&obj, "blue").is_none());
    }

    #[test]
    fn first_match_wins_on_duplicate_keys() {
        // Upstream uses path.stop() on first match — our forward walk
        // matches that semantic.
        let obj = parse_object_expression(r#"({ blue: 'first', blue: 'second' });"#);
        let r = get_object_property_value(&obj, "blue").expect("found");
        assert_eq!(expect_string_literal(&r.node), "first");
    }
}
