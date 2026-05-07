//! 1:1 port of `packages/babel-plugin/src/utils/css-map.ts`.
//!
//! This is the helper module shared between `css_map/index.ts`
//! (`visit_css_map_path`) and `css_map/process_selectors.ts`
//! (`merge_extended_selectors_into_properties`). It carries:
//!
//! - The `ErrorMessages` enum with the exact upstream phrasings.
//! - The `create_error_message` formatter (appends the documentation
//!   link suffix verbatim per upstream lines 94–100).
//! - The five literal-key / @-rule / extended-selectors helpers
//!   (`object_key_is_literal_value`, `get_key_value`,
//!   `is_at_rule_object`, `is_plain_selector`,
//!   `has_extended_selectors_key`).
//! - The `errorIfNotValidObjectProperty` predicate: returns `Err`
//!   on `ObjectMethod` / `SpreadElement`. Upstream uses an
//!   `asserts ... is t.ObjectProperty` type guard; the Rust port
//!   returns a Result so callers can `?` into the dispatcher's
//!   error channel.
//!
//! The upstream `EXTENDED_SELECTORS_KEY = 'selectors'` constant is
//! preserved as `EXTENDED_SELECTORS_KEY`.
//!
//! Drift watch points:
//! - The `atRules` Record in upstream uses `csstype` AtRules type
//!   keys verbatim. The Rust port inlines the same set of strings;
//!   any AT-rule the upstream csstype dependency adds must be
//!   mirrored here. Bumping the upstream `@compiled/babel-plugin`
//!   version triggers a recheck.
//! - `objectKeyIsLiteralValue` in upstream is a type guard
//!   (`key is ObjectKeyWithLiteralValue`). The Rust port returns
//!   `bool`; callers that need the discriminated value call
//!   `get_key_value` (which itself returns Option/String).

use swc_core::ecma::ast::{Expr, Lit, Prop, PropName, PropOrSpread};

use crate::utils::ast::{build_code_frame_error, CssBuildError};

pub const EXTENDED_SELECTORS_KEY: &str = "selectors";

/// Mirrors upstream's `atRules` Record. Keep this list in sync with
/// `csstype.AtRules` (the upstream typing source).
const AT_RULES: &[&str] = &[
    "@charset",
    "@counter-style",
    "@document",
    "@font-face",
    "@font-feature-values",
    "@font-palette-values",
    "@import",
    "@keyframes",
    "@layer",
    "@media",
    "@namespace",
    "@page",
    "@property",
    "@scope",
    "@scroll-timeline",
    "@starting-style",
    "@supports",
    "@viewport",
];

/// `objectKeyIsLiteralValue` upstream lines 32–34. Returns `true` for
/// `Identifier` / `StringLiteral` keys. Rust callers that need the
/// concrete string call [`get_key_value`].
///
/// SWC↔Babel parser delta: Babel's `t.isIdentifier(key)` and
/// `t.isStringLiteral(key)` operate on the AST node identity REGARDLESS
/// of `property.computed`. For `{ [foo]: 1 }`, Babel exposes
/// `property.key === Identifier(foo)` with `property.computed === true`,
/// and `t.isIdentifier(key)` returns `true`. SWC's parser distinguishes
/// the two: a non-computed identifier key becomes `PropName::Ident`,
/// a computed identifier key becomes `PropName::Computed { expr:
/// Expr::Ident(_) }`. To match Babel's predicate byte-for-byte, treat
/// computed-with-Ident-or-Str-inside as literal too. Same for
/// computed-with-StringLiteral-inside, which Babel's
/// `t.isStringLiteral(key)` would also return `true` for.
///
/// Surfaced by `ct-cssmap-massive` whose variant body uses
/// `[CURRENT_SURFACE_CSS_VAR]: '#FFFFFF'` (computed identifier key).
/// Babel accepted the prop as literal and returned the identifier
/// NAME from `getKeyValue` (`"CURRENT_SURFACE_CSS_VAR"`) — the variant
/// then proceeded through `process_selectors` because the name didn't
/// match any at-rule / `selectors` / pseudo-selector. We mirror that
/// by accepting the same shapes here and projecting the same string
/// (identifier name OR literal value) in `get_key_value`.
pub fn object_key_is_literal_value(key: &PropName) -> bool {
    match key {
        PropName::Ident(_) | PropName::Str(_) => true,
        PropName::Computed(c) => matches!(
            &*c.expr,
            Expr::Ident(_) | Expr::Lit(Lit::Str(_))
        ),
        _ => false,
    }
}

/// `getKeyValue` upstream lines 36–40. Mirrors the throw on
/// non-literal keys with an `Err` return — the upstream invariant is
/// that callers gate on `objectKeyIsLiteralValue` first.
///
/// See `object_key_is_literal_value` for the SWC↔Babel parser-delta
/// note: computed-Ident / computed-Str keys project the same string
/// Babel's `getKeyValue` would (the identifier name or string value).
pub fn get_key_value(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(id) => Some(id.sym.as_ref().to_string()),
        PropName::Str(s) => Some(s.value.to_atom_lossy().as_str().to_string()),
        PropName::Computed(c) => match &*c.expr {
            Expr::Ident(id) => Some(id.sym.as_ref().to_string()),
            Expr::Lit(Lit::Str(s)) => Some(s.value.to_atom_lossy().as_str().to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// `isAtRuleObject` upstream lines 42–47. Returns `true` if the key's
/// literal value is a member of [`AT_RULES`].
pub fn is_at_rule_object(key: &PropName) -> bool {
    let Some(v) = get_key_value(key) else {
        return false;
    };
    AT_RULES.iter().any(|r| *r == v.as_str())
}

/// `isPlainSelector` upstream line 49. Returns `true` if the selector
/// starts with `:`.
pub fn is_plain_selector(selector: &str) -> bool {
    selector.starts_with(':')
}

/// `hasExtendedSelectorsKey` upstream lines 51–52. Predicate over an
/// ObjectProperty's KEY: matches if it is a literal key whose value
/// is `"selectors"`.
pub fn has_extended_selectors_key(key: &PropName) -> bool {
    object_key_is_literal_value(key)
        && get_key_value(key).as_deref() == Some(EXTENDED_SELECTORS_KEY)
}

/// `errorIfNotValidObjectProperty` upstream lines 54–71. Returns
/// `Err` on `ObjectMethod` / `SpreadElement`; `Ok(())` for
/// `ObjectProperty`. Callers `?` into the dispatcher's error channel.
///
/// Upstream's TypeScript signature is an `asserts` type guard; the
/// Rust port returns `Result<(), CssBuildError>` so the call site
/// can propagate via `?`. The cluster of `is.../is_plain_selector/...`
/// helpers below all assume this guard has fired first.
pub fn error_if_not_valid_object_property(
    property: &PropOrSpread,
) -> Result<(), CssBuildError> {
    match property {
        PropOrSpread::Spread(spread) => Err(build_code_frame_error(
            create_error_message(ErrorMessages::NoSpreadElement.text()),
            Some(spread.dot3_token),
        )),
        PropOrSpread::Prop(p) => match p.as_ref() {
            // Object methods (`{ foo() {} }`).
            Prop::Method(m) => Err(build_code_frame_error(
                create_error_message(ErrorMessages::NoObjectMethod.text()),
                Some(m.key.span()),
            )),
            Prop::Getter(g) => Err(build_code_frame_error(
                create_error_message(ErrorMessages::NoObjectMethod.text()),
                Some(g.key.span()),
            )),
            Prop::Setter(s) => Err(build_code_frame_error(
                create_error_message(ErrorMessages::NoObjectMethod.text()),
                Some(s.key.span()),
            )),
            // Property shorthand / KeyValue → valid.
            Prop::Shorthand(_) | Prop::KeyValue(_) | Prop::Assign(_) => Ok(()),
        },
    }
}

/// `ErrorMessages` upstream lines 75–92. EXACT phrasings — these
/// strings are part of the byte contract for error frames so
/// production messages match across the JS and Rust pipelines.
#[derive(Debug, Clone, Copy)]
pub enum ErrorMessages {
    NoTaggedTemplate,
    NumberOfArgument,
    ArgumentType,
    AtRuleValueType,
    SelectorsBlockValueType,
    DefineMap,
    NoSpreadElement,
    NoObjectMethod,
    StaticVariantObject,
    DuplicateAtRule,
    DuplicateSelector,
    DuplicateSelectorsBlock,
    StaticPropertyKey,
    SelectorBlockWrongPlace,
    UseSelectorsWithAmpersand,
    UseVariantOfCssMap,
}

impl ErrorMessages {
    pub fn text(self) -> &'static str {
        use ErrorMessages::*;
        match self {
            NoTaggedTemplate => "cssMap function cannot be used as a tagged template expression.",
            NumberOfArgument => "cssMap function can only receive one argument.",
            ArgumentType => "cssMap function can only receive an object.",
            AtRuleValueType => "Value of at-rule block must be an object.",
            SelectorsBlockValueType => "Value of `selectors` key must be an object.",
            DefineMap => "CSS Map must be declared at the top-most scope of the module.",
            NoSpreadElement => "Spread element is not supported in CSS Map.",
            NoObjectMethod => "Object method is not supported in CSS Map.",
            StaticVariantObject => "The variant object must be statically defined.",
            DuplicateAtRule => "Cannot declare an at-rule more than once in CSS Map.",
            DuplicateSelector => "Cannot declare a selector more than once in CSS Map.",
            DuplicateSelectorsBlock => "Duplicate `selectors` key found in cssMap; expected either zero `selectors` keys or one.",
            StaticPropertyKey => "Property key may only be a static string.",
            SelectorBlockWrongPlace => "`selector` key was defined in the wrong place.",
            UseSelectorsWithAmpersand => "This selector is applied to the parent element, and so you need to specify the ampersand symbol (&) directly before it. For example, `:hover` should be written as `&:hover`.",
            UseVariantOfCssMap => "You must use the variant of a CSS Map object (eg. `styles.root`), not the root object itself, eg. `styles`.",
        }
    }
}

/// `createErrorMessage` upstream lines 94–100. Appends the
/// documentation-link suffix verbatim — the trailing newline shape
/// is part of the byte contract for error frames.
pub fn create_error_message(message: &str) -> String {
    format!(
        "\n{}\n\nCheck out our documentation for cssMap examples: https://compiledcssinjs.com/docs/api-cssmap\n",
        message
    )
}

// PropName::span() helper — SWC PropName doesn't expose .span() on
// the enum directly so we walk the variants. Used by the
// error_if_not_valid_object_property error span attachment.
trait PropNameSpan {
    fn span(&self) -> swc_core::common::Span;
}

impl PropNameSpan for PropName {
    fn span(&self) -> swc_core::common::Span {
        match self {
            PropName::Ident(i) => i.span,
            PropName::Str(s) => s.span,
            PropName::Num(n) => n.span,
            PropName::Computed(c) => c.span,
            PropName::BigInt(b) => b.span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{
        ComputedPropName, Expr, Ident, KeyValueProp, Number, Prop, PropOrSpread, SpreadElement,
        Str,
    };

    fn ident_key(name: &str) -> PropName {
        PropName::Ident(
            Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty()).into(),
        )
    }
    fn str_key(value: &str) -> PropName {
        PropName::Str(Str {
            span: DUMMY_SP,
            value: value.into(),
            raw: None,
        })
    }
    fn num_key(value: f64) -> PropName {
        PropName::Num(Number {
            span: DUMMY_SP,
            value,
            raw: None,
        })
    }
    fn computed_ident_key(name: &str) -> PropName {
        PropName::Computed(ComputedPropName {
            span: DUMMY_SP,
            expr: Box::new(Expr::Ident(Ident::new(
                name.into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            ))),
        })
    }
    fn computed_str_key(value: &str) -> PropName {
        PropName::Computed(ComputedPropName {
            span: DUMMY_SP,
            expr: Box::new(Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: value.into(),
                raw: None,
            }))),
        })
    }
    fn computed_call_key() -> PropName {
        PropName::Computed(ComputedPropName {
            span: DUMMY_SP,
            expr: Box::new(Expr::Call(swc_core::ecma::ast::CallExpr {
                span: DUMMY_SP,
                callee: swc_core::ecma::ast::Callee::Expr(Box::new(Expr::Ident(Ident::new(
                    "f".into(),
                    DUMMY_SP,
                    SyntaxContext::empty(),
                )))),
                args: vec![],
                type_args: None,
                ctxt: SyntaxContext::empty(),
            })),
        })
    }

    #[test]
    fn object_key_is_literal_value_classifies_correctly() {
        assert!(object_key_is_literal_value(&ident_key("foo")));
        assert!(object_key_is_literal_value(&str_key("foo")));
        assert!(!object_key_is_literal_value(&num_key(1.0)));
        // Babel's `t.isIdentifier(key)` / `t.isStringLiteral(key)` return
        // `true` for computed keys whose inner expression is an Identifier
        // or StringLiteral (the AST `key` node IS the inner Ident/Str
        // when `computed: true` in Babel). The Rust port matches that
        // shape here; non-Ident/non-Str computed exprs (calls, binops,
        // etc.) remain non-literal.
        assert!(object_key_is_literal_value(&computed_ident_key("x")));
        assert!(object_key_is_literal_value(&computed_str_key("x")));
        assert!(!object_key_is_literal_value(&computed_call_key()));
    }

    #[test]
    fn get_key_value_returns_string_for_ident_and_str() {
        assert_eq!(get_key_value(&ident_key("foo")).as_deref(), Some("foo"));
        assert_eq!(get_key_value(&str_key("bar")).as_deref(), Some("bar"));
        assert_eq!(get_key_value(&num_key(1.0)), None);
        // Computed-Ident projects the identifier NAME (mirrors Babel's
        // `getKeyValue` for `[foo]` returning `"foo"` — the literal text
        // of the identifier, NOT a resolved binding value).
        assert_eq!(
            get_key_value(&computed_ident_key("CURRENT_SURFACE_CSS_VAR")).as_deref(),
            Some("CURRENT_SURFACE_CSS_VAR")
        );
        assert_eq!(
            get_key_value(&computed_str_key("--ds-foo")).as_deref(),
            Some("--ds-foo")
        );
        assert_eq!(get_key_value(&computed_call_key()), None);
    }

    #[test]
    fn is_at_rule_object_recognises_canonical_at_rules() {
        assert!(is_at_rule_object(&str_key("@media")));
        assert!(is_at_rule_object(&ident_key("@media")));
        assert!(is_at_rule_object(&str_key("@supports")));
        assert!(is_at_rule_object(&str_key("@layer")));
        assert!(!is_at_rule_object(&str_key("@unknown-rule")));
        assert!(!is_at_rule_object(&str_key("color")));
    }

    #[test]
    fn is_plain_selector_detects_pseudo_prefix() {
        assert!(is_plain_selector(":hover"));
        assert!(is_plain_selector("::before"));
        assert!(!is_plain_selector("&:hover"));
        assert!(!is_plain_selector("div"));
    }

    #[test]
    fn has_extended_selectors_key_matches_only_literal_selectors_string() {
        assert!(has_extended_selectors_key(&ident_key("selectors")));
        assert!(has_extended_selectors_key(&str_key("selectors")));
        assert!(!has_extended_selectors_key(&ident_key("color")));
        assert!(!has_extended_selectors_key(&num_key(1.0)));
    }

    #[test]
    fn error_if_not_valid_object_property_accepts_keyvalue() {
        let prop = PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: ident_key("foo"),
            value: Box::new(Expr::Ident(Ident::new(
                "bar".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            ))),
        })));
        assert!(error_if_not_valid_object_property(&prop).is_ok());
    }

    #[test]
    fn error_if_not_valid_object_property_rejects_spread() {
        let prop = PropOrSpread::Spread(SpreadElement {
            dot3_token: DUMMY_SP,
            expr: Box::new(Expr::Ident(Ident::new(
                "x".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            ))),
        });
        let err = error_if_not_valid_object_property(&prop).unwrap_err();
        assert!(err.message.contains("Spread element"));
    }

    #[test]
    fn create_error_message_appends_documentation_link() {
        let out = create_error_message("hello");
        assert!(out.contains("hello"));
        assert!(out.contains("https://compiledcssinjs.com/docs/api-cssmap"));
        // Leading + trailing newline shape is part of the byte
        // contract; first char must be '\n'.
        assert!(out.starts_with('\n'));
        assert!(out.ends_with('\n'));
    }
}
