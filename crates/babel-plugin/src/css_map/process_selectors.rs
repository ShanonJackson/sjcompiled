//! 1:1 port of `packages/babel-plugin/src/css-map/process-selectors.ts`.
//!
//! Three exports mirror upstream:
//!
//! - `merge_extended_selectors_into_properties` (public) — collapses
//!   the `selectors: { ... }` shorthand into top-level keys, expands
//!   at-rule blocks (`@media: { 'screen ...': ..., ... }` → multiple
//!   `@media screen ...: ...` keys), and dedupes / errors on
//!   conflicts.
//! - `collapse_at_rule` (private generator) — yields each
//!   `(at_rule_name, at_rule_value)` pair from an at-rule block.
//! - `get_extended_selectors` (private) — extracts the single
//!   `selectors: { ... }` block (or `[]`) and validates the count
//!   (≤1).
//!
//! Drift watch points:
//! - Iteration order: upstream uses
//!   `[...variantStyles.properties, ...extendedSelectors]`. The Rust
//!   port preserves source order via `Vec::extend` over the input
//!   slice.
//! - The "skip the selectors-block property in the merge" branch
//!   (upstream line 143: `if (propertyHasExtendedSelectorsKey(...))
//!   continue`) is mirrored verbatim.
//! - Duplicate detection uses a `HashSet<String>` keyed on the
//!   stringified key. Upstream uses a `Set<string>` from
//!   `addedSelectors`; behaviour is bit-equal.

use std::collections::HashSet;

use swc_core::ecma::ast::{
    Expr, KeyValueProp, ObjectLit, Prop, PropName, PropOrSpread, Str,
};

use crate::utils::ast::{build_code_frame_error, CssBuildError};
use crate::utils::css_map::{
    create_error_message, error_if_not_valid_object_property, get_key_value,
    has_extended_selectors_key, is_at_rule_object, is_plain_selector,
    object_key_is_literal_value, ErrorMessages,
};

/// Helper: pull `&Vec<PropOrSpread>` out of a `Prop::KeyValue` whose
/// value is an `ObjectExpression`. Returns `None` if the value is not
/// an object expression.
fn prop_value_object_props(prop: &Prop) -> Option<&Vec<PropOrSpread>> {
    let Prop::KeyValue(kv) = prop else {
        return None;
    };
    let Expr::Object(obj) = kv.value.as_ref() else {
        return None;
    };
    Some(&obj.props)
}

/// Helper: pull the key out of a Prop::KeyValue.
fn prop_key(prop: &Prop) -> Option<&PropName> {
    if let Prop::KeyValue(kv) = prop {
        Some(&kv.key)
    } else {
        None
    }
}

/// `collapseAtRule` upstream lines 16–39. Yields each
/// `(at_rule_name, at_rule_value)` pair from an at-rule block.
///
/// Errors when:
/// - the at-rule block's value is not an ObjectExpression
///   (`AT_RULE_VALUE_TYPE`)
/// - an inner property is invalid (`error_if_not_valid_object_property`)
/// - an inner key is not a literal value (`STATIC_PROPERTY_KEY`)
fn collapse_at_rule(
    at_rule_block: &Prop,
    at_rule_type: &str,
) -> Result<Vec<(String, PropOrSpread)>, CssBuildError> {
    // upstream `if (!t.isObjectExpression(atRuleBlock.value))`
    let inner_props = prop_value_object_props(at_rule_block).ok_or_else(|| {
        // Anchor the error at the value's span if available; fall
        // back to a span-less error otherwise.
        let span = match at_rule_block {
            Prop::KeyValue(kv) => Some(swc_span(kv.value.as_ref())),
            _ => None,
        };
        build_code_frame_error(
            create_error_message(ErrorMessages::AtRuleValueType.text()),
            span,
        )
    })?;

    let mut out: Vec<(String, PropOrSpread)> = Vec::with_capacity(inner_props.len());

    for at_rule in inner_props {
        error_if_not_valid_object_property(at_rule)?;
        // After the guard, only KeyValue / Shorthand / Assign reach
        // here; collapseAtRule reads `atRule.key`, which only exists
        // on KeyValue. Shorthand / Assign aren't valid in an at-rule
        // block and would fail STATIC_PROPERTY_KEY downstream — for
        // 1:1 parity we mirror the upstream property-key access by
        // pattern-matching on KeyValue and treating non-KeyValue as
        // STATIC_PROPERTY_KEY.
        let PropOrSpread::Prop(boxed) = at_rule else {
            unreachable!("error_if_not_valid_object_property accepts only Prop");
        };
        let Prop::KeyValue(kv) = boxed.as_ref() else {
            return Err(build_code_frame_error(
                create_error_message(ErrorMessages::StaticPropertyKey.text()),
                None,
            ));
        };
        if !object_key_is_literal_value(&kv.key) {
            return Err(build_code_frame_error(
                create_error_message(ErrorMessages::StaticPropertyKey.text()),
                None,
            ));
        }

        // `${atRuleType} ${getKeyValue(atRule.key)}`
        let inner_value = get_key_value(&kv.key).expect("guard above");
        let at_rule_name = format!("{} {}", at_rule_type, inner_value);

        // `{ ...atRule, key: t.identifier(atRuleName) }` — upstream
        // builds an Identifier with the merged name. The Rust port
        // mirrors with a string-literal key (PropName::Str) rather
        // than Ident because the merged name (`@media screen and
        // (min-width: 500px)`) contains spaces and parens that an
        // Ident cannot hold; this is a known SWC-vs-Babel divergence
        // — Babel's Identifier accepts any string, SWC's Ident does
        // not. The downstream consumer (`build_css`) reads the key
        // via `get_key_value`, which returns the same string for
        // either Ident or Str — bit-equal observable behaviour.
        let new_key = PropName::Str(Str {
            span: kv.key_span_or_dummy(),
            value: at_rule_name.as_str().into(),
            raw: None,
        });
        let new_prop = PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: new_key,
            value: kv.value.clone(),
        })));
        out.push((at_rule_name, new_prop));
    }

    Ok(out)
}

/// `getExtendedSelectors` upstream lines 41–69. Returns the props
/// from the single `selectors: { ... }` block, or `[]` if none. Errors
/// on multiple `selectors:` blocks (`DUPLICATE_SELECTORS_BLOCK`) or a
/// `selectors:` value that isn't an object (`SELECTORS_BLOCK_VALUE_TYPE`).
fn get_extended_selectors<'a>(
    variant_styles: &'a ObjectLit,
) -> Result<Vec<PropOrSpread>, CssBuildError> {
    // Filter to ObjectProperties whose key is the literal "selectors".
    let mut found: Vec<&'a Prop> = Vec::new();
    for prop in &variant_styles.props {
        let PropOrSpread::Prop(p) = prop else {
            continue;
        };
        let Some(key) = prop_key(p.as_ref()) else {
            continue;
        };
        if has_extended_selectors_key(key) {
            found.push(p.as_ref());
        }
    }

    if found.is_empty() {
        return Ok(Vec::new());
    }
    if found.len() > 1 {
        // Upstream points the error at `extendedSelectorsFound[1]`.
        return Err(build_code_frame_error(
            create_error_message(ErrorMessages::DuplicateSelectorsBlock.text()),
            None,
        ));
    }

    let extended = found[0];
    let inner = prop_value_object_props(extended).ok_or_else(|| {
        build_code_frame_error(
            create_error_message(ErrorMessages::SelectorsBlockValueType.text()),
            None,
        )
    })?;
    Ok(inner.clone())
}

/// `mergeExtendedSelectorsIntoProperties` upstream lines 109–185.
///
/// Builds a fresh ObjectLit whose props are the variant's properties
/// + the inlined extended selectors, with at-rule blocks expanded
/// into one merged `@<type> <value>` key per inner property. Mutates
/// `addedSelectors` (a Set in upstream, HashSet here) to dedupe
/// across both selector and at-rule kinds.
pub fn merge_extended_selectors_into_properties(
    variant_styles: &ObjectLit,
) -> Result<ObjectLit, CssBuildError> {
    let extended = get_extended_selectors(variant_styles)?;

    let mut merged: Vec<PropOrSpread> = Vec::with_capacity(variant_styles.props.len());
    let mut added: HashSet<String> = HashSet::new();

    let mut all_props: Vec<&PropOrSpread> = Vec::with_capacity(variant_styles.props.len() + extended.len());
    all_props.extend(variant_styles.props.iter());
    all_props.extend(extended.iter());

    for property in all_props {
        // Type-check the prop kind first.
        error_if_not_valid_object_property(property)?;
        let PropOrSpread::Prop(boxed) = property else {
            unreachable!("guarded above");
        };
        // Shorthand / Assign reach here; upstream's path reads
        // `property.key` which only exists on KeyValue. The other
        // kinds aren't valid CSS-Map shapes and would fail
        // STATIC_PROPERTY_KEY downstream — emit it directly.
        let Prop::KeyValue(kv) = boxed.as_ref() else {
            return Err(build_code_frame_error(
                create_error_message(ErrorMessages::StaticPropertyKey.text()),
                None,
            ));
        };
        let property_key = &kv.key;

        // upstream: `if (!objectKeyIsLiteralValue(propertyKey))`
        if !object_key_is_literal_value(property_key) {
            return Err(build_code_frame_error(
                create_error_message(ErrorMessages::StaticPropertyKey.text()),
                None,
            ));
        }

        // upstream: `if (isPlainSelector(getKeyValue(propertyKey)))`
        let key_value = get_key_value(property_key).expect("literal-key guarded above");
        if is_plain_selector(&key_value) {
            return Err(build_code_frame_error(
                create_error_message(ErrorMessages::UseSelectorsWithAmpersand.text()),
                None,
            ));
        }

        // upstream: `if (propertyHasExtendedSelectorsKey(property)) continue;`
        // — already extracted into `extended`, so skip on the merge.
        if has_extended_selectors_key(property_key) {
            continue;
        }

        if is_at_rule_object(property_key) {
            let at_rule_type = key_value.clone();
            let at_rules = collapse_at_rule(boxed.as_ref(), &at_rule_type)?;

            for (at_rule_name, at_rule_value) in at_rules {
                if added.contains(&at_rule_name) {
                    return Err(build_code_frame_error(
                        create_error_message(ErrorMessages::DuplicateAtRule.text()),
                        None,
                    ));
                }
                merged.push(at_rule_value);
                added.insert(at_rule_name);
            }
        } else {
            // upstream: `const isSelector = t.isObjectExpression(property.value);`
            let is_selector = matches!(kv.value.as_ref(), Expr::Object(_));

            if is_selector {
                let already = added.contains(&key_value);
                if already {
                    return Err(build_code_frame_error(
                        create_error_message(ErrorMessages::DuplicateSelector.text()),
                        None,
                    ));
                }
                added.insert(key_value.clone());
            }

            merged.push(property.clone());
        }
    }

    // upstream: `return { ...variantStyles, properties: mergedProperties };`
    Ok(ObjectLit {
        span: variant_styles.span,
        props: merged,
    })
}

// SWC's `KeyValueProp` doesn't expose `key.span()` ergonomically; the
// `Expr` value's span is what we want for AT_RULE_VALUE_TYPE. Helper
// trait keeps the call sites readable.
trait KeyValuePropExt {
    fn key_span_or_dummy(&self) -> swc_core::common::Span;
}

impl KeyValuePropExt for KeyValueProp {
    fn key_span_or_dummy(&self) -> swc_core::common::Span {
        match &self.key {
            PropName::Ident(i) => i.span,
            PropName::Str(s) => s.span,
            PropName::Num(n) => n.span,
            PropName::Computed(c) => c.span,
            PropName::BigInt(b) => b.span,
        }
    }
}

fn swc_span(expr: &Expr) -> swc_core::common::Span {
    match expr {
        Expr::Object(o) => o.span,
        Expr::Lit(l) => match l {
            swc_core::ecma::ast::Lit::Str(s) => s.span,
            swc_core::ecma::ast::Lit::Num(n) => n.span,
            swc_core::ecma::ast::Lit::Bool(b) => b.span,
            swc_core::ecma::ast::Lit::Null(n) => n.span,
            swc_core::ecma::ast::Lit::BigInt(b) => b.span,
            swc_core::ecma::ast::Lit::Regex(r) => r.span,
            swc_core::ecma::ast::Lit::JSXText(j) => j.span,
        },
        Expr::Ident(i) => i.span,
        _ => swc_core::common::DUMMY_SP,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{Ident, KeyValueProp, ObjectLit, Prop, PropName, PropOrSpread, Str};

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
    fn str_value(value: &str) -> Box<Expr> {
        Box::new(Expr::Lit(swc_core::ecma::ast::Lit::Str(Str {
            span: DUMMY_SP,
            value: value.into(),
            raw: None,
        })))
    }
    fn key_value(k: PropName, v: Box<Expr>) -> PropOrSpread {
        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: k,
            value: v,
        })))
    }
    fn object_value(props: Vec<PropOrSpread>) -> Box<Expr> {
        Box::new(Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props,
        }))
    }

    #[test]
    fn empty_variant_returns_empty_merged() {
        let v = ObjectLit { span: DUMMY_SP, props: vec![] };
        let out = merge_extended_selectors_into_properties(&v).unwrap();
        assert!(out.props.is_empty());
    }

    #[test]
    fn flat_property_passes_through() {
        let v = ObjectLit {
            span: DUMMY_SP,
            props: vec![key_value(ident_key("color"), str_value("red"))],
        };
        let out = merge_extended_selectors_into_properties(&v).unwrap();
        assert_eq!(out.props.len(), 1);
    }

    #[test]
    fn extended_selectors_inline_lifted_to_top_level() {
        // { color: 'red', selectors: { div: { color: 'blue' } } }
        let inner_div = key_value(
            ident_key("div"),
            object_value(vec![key_value(ident_key("color"), str_value("blue"))]),
        );
        let selectors_block = key_value(ident_key("selectors"), object_value(vec![inner_div]));
        let v = ObjectLit {
            span: DUMMY_SP,
            props: vec![
                key_value(ident_key("color"), str_value("red")),
                selectors_block,
            ],
        };
        let out = merge_extended_selectors_into_properties(&v).unwrap();
        // After the lift: { color: 'red', div: { color: 'blue' } }
        // — the `selectors` block itself is dropped.
        assert_eq!(out.props.len(), 2);
        let keys: Vec<String> = out
            .props
            .iter()
            .filter_map(|p| match p {
                PropOrSpread::Prop(b) => match b.as_ref() {
                    Prop::KeyValue(kv) => get_key_value(&kv.key),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(keys, vec!["color".to_string(), "div".to_string()]);
    }

    #[test]
    fn at_rule_block_expands_into_one_key_per_inner() {
        // @media: {
        //   'screen and (min-width: 500px)': { color: 'red' },
        //   'screen and (min-width: 700px)': { color: 'blue' },
        // }
        let inner1 = key_value(
            str_key("screen and (min-width: 500px)"),
            object_value(vec![key_value(ident_key("color"), str_value("red"))]),
        );
        let inner2 = key_value(
            str_key("screen and (min-width: 700px)"),
            object_value(vec![key_value(ident_key("color"), str_value("blue"))]),
        );
        let media_block = key_value(str_key("@media"), object_value(vec![inner1, inner2]));
        let v = ObjectLit {
            span: DUMMY_SP,
            props: vec![media_block],
        };
        let out = merge_extended_selectors_into_properties(&v).unwrap();
        assert_eq!(out.props.len(), 2);
        let keys: Vec<String> = out
            .props
            .iter()
            .filter_map(|p| match p {
                PropOrSpread::Prop(b) => match b.as_ref() {
                    Prop::KeyValue(kv) => get_key_value(&kv.key),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(
            keys,
            vec![
                "@media screen and (min-width: 500px)".to_string(),
                "@media screen and (min-width: 700px)".to_string(),
            ]
        );
    }

    #[test]
    fn duplicate_at_rule_errors() {
        // @media: { 'q': {...} } AND @media: { 'q': {...} } — but the
        // second one is from a different block. Easier: collapse @media
        // with the same inner key twice via a single block (rare in
        // practice but exercises the dedupe).
        let inner_a = key_value(
            str_key("screen"),
            object_value(vec![key_value(ident_key("color"), str_value("red"))]),
        );
        let inner_b = key_value(
            str_key("screen"),
            object_value(vec![key_value(ident_key("color"), str_value("blue"))]),
        );
        let media_block = key_value(str_key("@media"), object_value(vec![inner_a, inner_b]));
        let v = ObjectLit {
            span: DUMMY_SP,
            props: vec![media_block],
        };
        let err = merge_extended_selectors_into_properties(&v).unwrap_err();
        assert!(err.message.contains("at-rule"));
    }

    #[test]
    fn duplicate_selector_errors() {
        // { div: {...}, div: {...} }
        let div1 = key_value(
            ident_key("div"),
            object_value(vec![key_value(ident_key("color"), str_value("red"))]),
        );
        let div2 = key_value(
            ident_key("div"),
            object_value(vec![key_value(ident_key("color"), str_value("blue"))]),
        );
        let v = ObjectLit {
            span: DUMMY_SP,
            props: vec![div1, div2],
        };
        let err = merge_extended_selectors_into_properties(&v).unwrap_err();
        assert!(err.message.contains("selector"));
    }

    #[test]
    fn duplicate_selectors_block_errors() {
        // { selectors: {}, selectors: {} }
        let s1 = key_value(ident_key("selectors"), object_value(vec![]));
        let s2 = key_value(ident_key("selectors"), object_value(vec![]));
        let v = ObjectLit {
            span: DUMMY_SP,
            props: vec![s1, s2],
        };
        let err = merge_extended_selectors_into_properties(&v).unwrap_err();
        assert!(err.message.contains("Duplicate `selectors` key"));
    }

    #[test]
    fn plain_selector_without_ampersand_errors() {
        // { ':hover': {...} } — must use `&:hover`.
        let hover = key_value(
            str_key(":hover"),
            object_value(vec![key_value(ident_key("color"), str_value("red"))]),
        );
        let v = ObjectLit {
            span: DUMMY_SP,
            props: vec![hover],
        };
        let err = merge_extended_selectors_into_properties(&v).unwrap_err();
        assert!(err.message.contains("ampersand"));
    }
}
