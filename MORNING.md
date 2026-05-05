● §6.8f complete. Summary:

Three coordinated landings closing the ternary-with-literal-branch defect:
1. utils/manipulate_template_literal.rs::has_nested_template_literals_with_conditional_rules — was unimplemented!() per Phase 5 §5.6 stub; replaced with a recursive within-Tpl walker covering upstream's case 2 (nested
templates with arrow-body interp) and case 3 (logical exprs in interpolation), recursing through Cond.test/cons/alt, Arrow.body, and Paren.expr.                                                                                    2. Bug-parity fix in optimize_conditional_statement: unwrap_paren before Expr::Cond(_) pattern-match (Babel parser strips paren; SWC keeps it). Fixed both the body_is_conditional gate and the original_cond extraction. Mirrors
§6.8c-#1 paren-shim convention.
3. utils/css_builders.rs integration: wires optimize_conditional_statement into extract_template_literal's gate with synthetic TplElement shells (since SWC walks AST by & ref, no in-place mutation channel like Babel's
   NodePath). Defensive cond_has_literal_branches narrowing avoids a foreign-import panic that surfaces when non-literal branches (colors.N20) get Tpl-wrapped and re-enter extract_template_literal recursively.
4. Metadata::in_conditional_branch flag (new field threaded through 30+ call sites). Set by extract_branch before recursive build_css_inner(Tpl/TaggedTpl/Call) calls; gates the inner optimization so
   nested-in-conditional-branch templates skip per-interpolation optimization (mirrors upstream's case-1 detection from hasNestedTemplateLiteralsWithConditionalRules without requiring §5.6's parent-walk).

Triage delta: parity 313 → 313 (steady), divergence 163 → 163 (steady), swc-throws 0 → 0 (steady), lib tests 452 → 452. Net cluster count is unchanged because every affected fixture in styled/* clusters ALSO needs the
prop-destructure feature (§6.8g — Babel's const { isRounded, ...__cmpldp } = __cmplp;). The CSS bytes my fix produces are now correct (verified via inspect-one); the surrounding component code still differs by the unfiltered  
spread.

§6.8g (next) — port the prop-destructure-for-consumed-props feature in build_styled_component.rs: collect prop names referenced by conditional CSS / runtime ternary-className expressions, destructure them out of __cmplp into  
__cmpldp, swap the JSX spread to ...__cmpldp. Affects styled/behaviour (~30+), styled/call-expression (~16), styled/tagged-template-expression (~8) — likely the largest remaining single follow-up.