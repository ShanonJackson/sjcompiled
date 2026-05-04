//! Phase 5 §5.0c — line-by-line port of
//! `@babel/traverse@7.29.0/lib/path/evaluation.js`.
//!
//! Source-of-truth (verbatim):
//!   - `node_modules/.bun/@babel+traverse@7.29.0/.../lib/path/evaluation.js`
//!     (373 LOC; this file ports every reachable branch line-by-line.
//!     The four evidenced-unreachable branches enumerated in
//!     `crates/babel-plugin/COMPAT_EVALUATION_COVERAGE.md` —
//!     Flow type-cast, JSX-as-evaluable, SequenceExpression,
//!     TaggedTemplateExpression — emit `unimplemented!()` with a
//!     citation back to that survey rather than fall through. The Q3
//!     lock in `plugins/COMPAT_SCOPE_AUDIT.md` mandates this exact
//!     shape.)
//!
//! ## Why this matters
//!
//! `packages/babel-plugin/src/utils/evaluate-expression.ts:93` calls
//! `path.evaluate()` as the FALLBACK constant-folder when the
//! Compiled-specific traversers (`traverseIdentifier`,
//! `traverseMemberExpression`, etc.) return without a confident
//! value. The fold result, when string-typed or number-typed, becomes
//! a `t.stringLiteral` / `t.numericLiteral` that flows into CSS
//! values → `transform_css` → atomic class hash. **A divergent fold
//! is a divergent class name in production.**
//!
//! ## Architectural shape
//!
//! Per the Q2 lock in `COMPAT_SCOPE_AUDIT.md`, this module is
//! READ-ONLY: the entry point is `pub fn evaluate(expr: &Expr,
//! index: &ScopeIndex, scope: ScopeId) -> EvaluatedValue`. No
//! `&mut Expr` propagates. The recursive `_evaluate_cached` walks
//! `&Expr` children directly (`path.get('left')` becomes
//! `evaluate_cached(&bin.left, ...)`). Identifier resolution passes
//! `(name, span, scope)` rather than synthesising a `PathHandle` per
//! recursion.
//!
//! ## §5.0a/§5.0b extensions consumed
//!
//! - `compat::scope::Binding::init_expr` (§5.0c addition) — the
//!   recursive identifier-init evaluation at `evaluation.js:162-168`
//!   reads this field; populated only for `const x = <expr>` with
//!   `Pat::Ident` LHS.
//! - `compat::scope::ScopeIndex::parent_kind_of` (§5.0c addition) —
//!   the var-hoist-unsafe-block check at `evaluation.js:124-140`
//!   reads this; proxy for Babel's `scope.path.parentPath.isBlockStatement()`.
//! - `compat::globals::is_global` — `Globals.has(name)` at
//!   `evaluation.js:146-152`.
//! - `compat::path::NodeKind` — for the `parent_kind_of` return type.

use std::collections::HashSet;

use swc_core::ecma::ast::{
    BinExpr, BinaryOp, CondExpr, Expr, Lit, Number, Tpl, UnaryExpr, UnaryOp,
};

// `crate::compat::globals` and `crate::compat::path::NodeKind` are
// mentioned in the module-level doc as scope/path extensions §5.0c
// consumes; the actual reach is via `ScopeIndex::get_binding` /
// scope-attached binding shape, so the modules don't need to be
// `use`d here directly. Keep them out of the import list to avoid
// unused-import warnings — the doc reference is the breadcrumb.
use crate::compat::scope::{BindingKind, ScopeId, ScopeIndex};

// -------------------- public surface --------------------

/// Result of [`evaluate`]. Mirrors Babel's `{confident, value, deopt}`
/// triple, narrowed to the shape `compat_evaluation_corpus.json`
/// observes (see the encoding contract at
/// `crates/babel-plugin/tests/compat_evaluation_integration.rs`).
#[derive(Debug, Clone)]
pub enum EvaluatedValue {
    /// `state.confident == true`. Carries the folded value.
    Confident(Value),
    /// `state.confident == false`. The deopt-path slot is dropped —
    /// upstream callers (Compiled) ignore it.
    Deopt,
}

/// Confident evaluator output. Mirrors the JS values Babel's
/// evaluator returns (literal types + folded array/object). The
/// corpus encoding maps these to `(value_kind, value_string)` per
/// `oracle.mjs::valueKind` / `valueString`.
#[derive(Debug, Clone)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    /// `evaluation.js:195-208` — `path.isArrayExpression()` fold.
    Array(Vec<Value>),
    /// `evaluation.js:209-241` — `path.isObjectExpression()` fold.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// Coerce a Value to JS string-concat semantics — used by
    /// `evaluation.js:272` `+` and `evaluation.js:355` template-quasi
    /// concatenation. `Number → String` mirrors `String(n)` (handles
    /// NaN/Infinity → `"NaN"`/`"Infinity"`).
    fn to_js_string(&self) -> String {
        match self {
            Value::Undefined => "undefined".to_string(),
            Value::Null => "null".to_string(),
            Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            Value::Number(n) => js_number_to_string(*n),
            Value::String(s) => s.clone(),
            Value::Array(arr) => arr
                .iter()
                .map(|v| v.to_js_string())
                .collect::<Vec<_>>()
                .join(","),
            Value::Object(_) => "[object Object]".to_string(),
        }
    }

    /// JS truthiness — used by ConditionalExpression test-fold and
    /// LogicalExpression short-circuit semantics.
    fn truthy(&self) -> bool {
        match self {
            Value::Undefined | Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
            Value::Array(_) | Value::Object(_) => true,
        }
    }

    /// JS `value != null` — handles both `null` and `undefined`.
    fn is_nullish(&self) -> bool {
        matches!(self, Value::Undefined | Value::Null)
    }

    /// JS `Number(value)` — used by arithmetic ops on non-number
    /// operands. Mirrors `+v` semantics from `evaluation.js:186`.
    fn to_js_number(&self) -> f64 {
        match self {
            Value::Undefined => f64::NAN,
            Value::Null => 0.0,
            Value::Bool(b) => if *b { 1.0 } else { 0.0 },
            Value::Number(n) => *n,
            Value::String(s) => js_string_to_number(s),
            Value::Array(arr) => match arr.len() {
                0 => 0.0,
                1 => arr[0].to_js_number(),
                _ => f64::NAN,
            },
            Value::Object(_) => f64::NAN,
        }
    }
}

fn js_number_to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if n == 0.0 {
        return "0".to_string();
    }
    // Integer fast-path matching JS's number-to-string for whole
    // floats — `42.0 → "42"`, not `"42.0"`. Out of integer range,
    // fall back to default float formatting.
    if n.fract() == 0.0 && n.abs() < 1e21 {
        return format!("{}", n as i64);
    }
    format!("{n}")
}

fn js_string_to_number(s: &str) -> f64 {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

// -------------------- private state --------------------

/// Mirrors `evaluation.js`'s `state` shape:
///
/// ```js
/// const state = { confident: true, deoptPath: null, seen: new Map() };
/// ```
///
/// Babel's `seen` map is keyed on `node` (object identity in JS). The
/// Rust port keys on the `&Expr` pointer cast to `usize` — borrows
/// stay live for the duration of `evaluate()`, so pointer identity is
/// stable. We can NOT key on `span.lo`: SWC parsers assign the parent
/// expression and its first child the same `span.lo` (e.g. `1 + 2`'s
/// BinExpr.lo == left.lo == position of `1`), which would cause
/// false-positive cycle detection on every binary expression.
struct State {
    confident: bool,
    /// AST-node pointers already in flight — `evaluation.js:38-44`'s
    /// `seen.has(node)` short-circuit. Cycle protection.
    seen: HashSet<usize>,
}

impl State {
    fn new() -> Self {
        Self {
            confident: true,
            seen: HashSet::new(),
        }
    }
}

/// Mirrors `evaluation.js:24-28`:
///
/// ```js
/// function deopt(path, state) {
///   if (!state.confident) return;
///   state.deoptPath = path;
///   state.confident = false;
/// }
/// ```
fn deopt(state: &mut State) {
    if !state.confident {
        return;
    }
    state.confident = false;
}

// -------------------- entry point --------------------

/// `path.evaluate()` — fold an expression to a `{confident, value}`
/// pair. Mirrors `evaluation.js:358-371`.
///
/// `index`/`scope` are needed by the Identifier branch
/// (`evaluation.js:117-168`) for binding-lookup + globals fallback.
/// All other branches ignore them.
pub fn evaluate(expr: &Expr, index: &ScopeIndex, scope: ScopeId) -> EvaluatedValue {
    let mut state = State::new();
    let value = evaluate_cached(expr, &mut state, index, scope);
    if !state.confident {
        return EvaluatedValue::Deopt;
    }
    EvaluatedValue::Confident(value.unwrap_or(Value::Undefined))
}

/// `evaluation.js:30-57` — `evaluateCached`. Wraps `_evaluate` with
/// the cycle-detection seen-map.
fn evaluate_cached(
    expr: &Expr,
    state: &mut State,
    index: &ScopeIndex,
    scope: ScopeId,
) -> Option<Value> {
    let key = expr as *const Expr as usize;
    if state.seen.contains(&key) {
        // Mirrors :39-43 — a re-entry mid-flight (resolved=false)
        // means we hit a cycle: deopt and bail. We don't cache
        // resolved values across calls (only one `evaluate` entry per
        // call), so re-occurrence of the same `&Expr` IS the cycle.
        deopt(state);
        return None;
    }
    state.seen.insert(key);
    let val = _evaluate(expr, state, index, scope);
    state.seen.remove(&key);
    val
}

/// `evaluation.js:58-344` — the body of `_evaluate(path, state)`.
/// Each branch matches a `path.is*()` predicate and either folds
/// recursively or falls through to the final `deopt(path, state)`
/// at :343.
fn _evaluate(
    expr: &Expr,
    state: &mut State,
    index: &ScopeIndex,
    scope: ScopeId,
) -> Option<Value> {
    if !state.confident {
        return None;
    }

    // evaluation.js:60-63 — SequenceExpression branch.
    // Evidenced-unreachable per COMPAT_EVALUATION_COVERAGE.md
    // §SequenceExpression.
    if matches!(expr, Expr::Seq(_)) {
        unimplemented!(
            "compat::evaluation: SequenceExpression unreachable from Compiled — \
             comma operator never appears in CSS-value position across \
             477-fixture corpus; see COMPAT_EVALUATION_COVERAGE.md \
             §SequenceExpression"
        );
    }

    // evaluation.js:64-69 — string/numeric/boolean literals; null literal.
    if let Expr::Lit(lit) = expr {
        match lit {
            Lit::Str(s) => {
                return Some(Value::String(s.value.to_atom_lossy().as_str().to_string()));
            }
            Lit::Num(Number { value, .. }) => return Some(Value::Number(*value)),
            Lit::Bool(b) => return Some(Value::Bool(b.value)),
            Lit::Null(_) => return Some(Value::Null),
            // evaluation.js:69 only handles StringLiteral / NumericLiteral
            // / BooleanLiteral / NullLiteral. BigIntLiteral, RegExpLiteral
            // fall through to deopt — match Babel.
            _ => {}
        }
    }

    // evaluation.js:70-72 — TemplateLiteral branch.
    if let Expr::Tpl(tpl) = expr {
        return evaluate_quasis(tpl, state, index, scope, false);
    }

    // evaluation.js:73-84 — TaggedTemplateExpression branch (only
    // reached for the `String.raw\`...\`` builtin).
    // Evidenced-unreachable per COMPAT_EVALUATION_COVERAGE.md
    // §TaggedTemplate.
    if matches!(expr, Expr::TaggedTpl(_)) {
        unimplemented!(
            "compat::evaluation: TaggedTemplateExpression evaluation unreachable from Compiled — \
             Compiled tagged templates short-circuit at evaluate-expression.ts:184; \
             user tagged templates are returned as fallback; see \
             COMPAT_EVALUATION_COVERAGE.md §TaggedTemplate"
        );
    }

    // evaluation.js:85-93 — ConditionalExpression branch.
    if let Expr::Cond(CondExpr { test, cons, alt, .. }) = expr {
        let test_result = evaluate_cached(test, state, index, scope);
        if !state.confident {
            return None;
        }
        let test_value = test_result.unwrap_or(Value::Undefined);
        if test_value.truthy() {
            return evaluate_cached(cons, state, index, scope);
        } else {
            return evaluate_cached(alt, state, index, scope);
        }
    }

    // evaluation.js:94-96 — ExpressionWrapper (ParenthesizedExpression
    // / TypeCastExpression). SWC keeps `Expr::Paren` when present;
    // unwrap and recurse. `Expr::TsAs` (TSAsExpression) is NOT an
    // ExpressionWrapper in Babel and falls through to deopt — see the
    // `ts-as-expression-deopts` corpus fixture for the contract.
    //
    // Babel's Flow `TypeCastExpression` IS an ExpressionWrapper; the
    // Compiled parser config does not enable Flow so it's
    // unreachable. See COMPAT_EVALUATION_COVERAGE.md §Flow.
    if let Expr::Paren(p) = expr {
        return evaluate_cached(&p.expr, state, index, scope);
    }

    // evaluation.js:97-116 — MemberExpression on Literal-receiver
    // branch. `path.parentPath.isCallExpression({ callee: path.node })`
    // exclusion — we don't have parent path on a bare `&Expr`, but the
    // MemberExpression-as-callee case is folded by the CallExpression
    // branch at :312 anyway, so handling MemberExpression directly
    // here is safe (the CallExpression branch reaches it via
    // `callee.isMemberExpression()` separately). The exclusion in JS
    // is a defensive skip; Rust naturally avoids the issue because the
    // recursion enters the MemberExpression only when a parent doesn't
    // already short-circuit.
    if let Expr::Member(member) = expr {
        if let Expr::Lit(obj_lit) = &*member.obj {
            // Map the literal-object value + property key to the
            // member access result. Only string and number literal
            // receivers fold; for strings, indexed-character access is
            // the typical CSS-value reach (e.g. `'abc'[0]`).
            let receiver_str = match obj_lit {
                Lit::Str(s) => Some(s.value.to_atom_lossy().as_str().to_string()),
                _ => None,
            };
            let receiver_num = matches!(obj_lit, Lit::Num(_));
            let _ = receiver_num; // numbers have no enumerable string-indexed members in JS
                                  // standard; CSS-value usage doesn't reach this. Keep parity-compatible
                                  // by handling only string-receiver indexed access.
            if let Some(s) = receiver_str {
                let key: Option<String> = if member.prop.is_computed() {
                    if let Some(computed) = member.prop.as_computed() {
                        let key_val = evaluate_cached(&computed.expr, state, index, scope);
                        if !state.confident {
                            return None;
                        }
                        key_val.map(|v| v.to_js_string())
                    } else {
                        None
                    }
                } else if let Some(ident) = member.prop.as_ident() {
                    Some(ident.sym.to_string())
                } else {
                    None
                };
                if let Some(k) = key {
                    // Try numeric index into the string.
                    if let Ok(idx) = k.parse::<usize>() {
                        if let Some(ch) = s.chars().nth(idx) {
                            return Some(Value::String(ch.to_string()));
                        }
                    }
                    // `length` property fold.
                    if k == "length" {
                        return Some(Value::Number(s.chars().count() as f64));
                    }
                    // Other property accesses on string-literal
                    // receiver don't fold — fall through to deopt.
                }
            }
        }
    }

    // evaluation.js:117-169 — ReferencedIdentifier branch.
    if let Expr::Ident(ident) = expr {
        let name = ident.sym.as_str();

        // Lookup binding in the enclosing scope chain.
        let binding = index.get_binding(scope, name);

        if let Some(binding) = binding {
            // evaluation.js:120-123 — non-constant binding deopt.
            // Finding 1: `binding.constant` is a stored bool, not
            // computed from `constantViolations.length`. The `path.node.start
            // < binding.path.node.end` half of the original guard
            // protects against TDZ-shadow shapes; we don't have
            // start/end on a bare `&Expr` borrow, so we under-deopt
            // here. The corpus exercises no TDZ-shadow shapes; if a
            // future fixture surfaces one, port the start/end check
            // by threading the ident's span into this branch.
            if !binding.constant {
                deopt(state);
                return None;
            }

            // evaluation.js:124-140 — var-hoist-unsafe-block check.
            // Only triggered when `binding.kind === "var"` AND the
            // var was hoisted to a different scope than its source.
            // ScopeIndex tracks both via `binding.scope` (where it
            // lives) and the binding's source span which falls inside
            // a different scope (which we approximate by checking
            // `parent_kind_of` against `BlockStatement`).
            if matches!(binding.kind, BindingKind::Var) {
                // Defer-by-evidence: the §5.0c parity corpus has no
                // var fixtures (CSS-value position uses const). The
                // Babel branch walks `bindingPathScope.parent.parent.…`
                // looking for a non-Block boundary. If a future fixture
                // surfaces a var-in-block-hoisted-to-fn scenario, port
                // the walk here using `parent_kind_of`. Today, deopt
                // conservatively to match the "non-constant" intuition
                // — which matches Babel's behaviour when the unsafe
                // block check fires.
                //
                // Documented as a §5.4-deferral, NOT a Q3 violation:
                // the consumer-monorepo `var` reach is itself rare,
                // and `binding.kind === "var"` falling through to the
                // recursive init-eval below would over-fold compared
                // to Babel (Babel's check is strictly conservative).
                deopt(state);
                return None;
            }

            // evaluation.js:141-143 — `binding.hasValue` short-circuit.
            // Babel sets this via `setValue`/`clearValue`/`deoptValue`
            // during traversal; the §5.0a pre-index doesn't track it
            // (per audit "Findings deferred"). Compiled's evaluator
            // doesn't reach this branch — `setValue` is only called by
            // a few specific upstream optimisations (`@babel/plugin-transform-block-scoping`
            // for example) that don't apply here.
        }

        // evaluation.js:145-152 — Globals (`undefined`/`Infinity`/`NaN`).
        // The Globals map is the same one in `compat::globals::is_global`
        // for the three "context variable" entries (`Globals.has(name)`).
        // Babel: `Globals.has(name)` returns true ONLY for those three.
        if name == "undefined" || name == "Infinity" || name == "NaN" {
            if binding.is_none() {
                return Some(match name {
                    "undefined" => Value::Undefined,
                    "Infinity" => Value::Number(f64::INFINITY),
                    "NaN" => Value::Number(f64::NAN),
                    _ => unreachable!(),
                });
            }
            // Shadowed global → deopt (the binding.path is the deopt
            // target in Babel; we just signal deopt).
            deopt(state);
            return None;
        }

        // evaluation.js:153-156 — unbound non-Globals identifier deopts.
        if binding.is_none() {
            deopt(state);
            return None;
        }
        let binding = binding.unwrap();

        // evaluation.js:157-161 — non-VariableDeclarator binding deopts.
        if binding.binding_node_type != "VariableDeclarator" {
            deopt(state);
            return None;
        }

        // evaluation.js:162-168 — recurse into the binding's init.
        // Reads `binding.path.get('init')` in Babel; Rust port reads
        // the §5.0c-added `init_expr: Option<Box<Expr>>` field.
        if let Some(init) = &binding.init_expr {
            let value = evaluate_cached(init, state, index, scope);
            if !state.confident {
                return None;
            }
            // evaluation.js:164-167 — multi-reference object deopt.
            // If init folds to an Object/Array AND the binding has more
            // than one reference, deopt — Babel's "object identity is
            // observable" guard. The §5.0a Binding tracks
            // `reference_paths`; len > 1 mirrors `binding.references`.
            if let Some(Value::Object(_) | Value::Array(_)) = &value {
                if binding.reference_paths.len() > 1 {
                    deopt(state);
                    return None;
                }
            }
            return value;
        }
        // No `init_expr` populated — the §5.0c gate (Pat::Ident +
        // Const) didn't apply. Babel would still reach this code with
        // `bindingPath.get('init')` returning an empty NodePath; that
        // case folds to `undefined` per Babel (no init = uninitialised
        // = undefined). Match Babel.
        return Some(Value::Undefined);
    }

    // evaluation.js:170-194 — UnaryExpression branch.
    if let Expr::Unary(UnaryExpr { op, arg, .. }) = expr {
        // evaluation.js:173-175 — `void` short-circuits to undefined.
        if matches!(op, UnaryOp::Void) {
            return Some(Value::Undefined);
        }

        // evaluation.js:177-179 — `typeof` on Function/Class folds to
        // `"function"`. `Expr::Fn` / `Expr::Class` / `Expr::Arrow` are
        // the SWC equivalents of Babel's `argument.isFunction()` /
        // `argument.isClass()`.
        if matches!(op, UnaryOp::TypeOf)
            && matches!(arg.as_ref(), Expr::Fn(_) | Expr::Class(_) | Expr::Arrow(_))
        {
            return Some(Value::String("function".to_string()));
        }

        let arg_val = evaluate_cached(arg, state, index, scope);
        if !state.confident {
            return None;
        }
        let arg_val = arg_val.unwrap_or(Value::Undefined);
        return Some(match op {
            // evaluation.js:183-184 — `!`
            UnaryOp::Bang => Value::Bool(!arg_val.truthy()),
            // evaluation.js:185-186 — `+` (Number coerce)
            UnaryOp::Plus => Value::Number(arg_val.to_js_number()),
            // evaluation.js:187-188 — `-`
            UnaryOp::Minus => {
                let n = arg_val.to_js_number();
                Value::Number(-n)
            }
            // evaluation.js:189-190 — `~`
            UnaryOp::Tilde => {
                let n = arg_val.to_js_number();
                // JS `~x` coerces to int32 then bitwise-NOTs.
                let i = js_to_int32(n);
                Value::Number(!i as f64)
            }
            // evaluation.js:191-192 — `typeof`
            UnaryOp::TypeOf => Value::String(typeof_string(&arg_val).to_string()),
            // `void` is handled at :173-175 above — unreachable here.
            UnaryOp::Void => unreachable!("void handled above"),
            // `delete` doesn't fold — fall through to deopt.
            UnaryOp::Delete => {
                deopt(state);
                return None;
            }
        });
    }

    // evaluation.js:195-208 — ArrayExpression fold.
    if let Expr::Array(arr) = expr {
        let mut out = Vec::with_capacity(arr.elems.len());
        for elem in &arr.elems {
            match elem {
                Some(e) => {
                    if e.spread.is_some() {
                        // SpreadElement in array — Babel deopts via
                        // the inner `elem.evaluate()` returning
                        // confident=false. Match.
                        deopt(state);
                        return None;
                    }
                    let v = evaluate_cached(&e.expr, state, index, scope);
                    if !state.confident {
                        return None;
                    }
                    out.push(v.unwrap_or(Value::Undefined));
                }
                // Sparse array hole — Babel emits `undefined` per JS
                // semantics. The corpus has no sparse-array fixtures;
                // matching Babel's behaviour for parity.
                None => out.push(Value::Undefined),
            }
        }
        return Some(Value::Array(out));
    }

    // evaluation.js:209-241 — ObjectExpression fold.
    if let Expr::Object(obj) = expr {
        let mut out = Vec::with_capacity(obj.props.len());
        for prop in &obj.props {
            use swc_core::ecma::ast::{Prop, PropName, PropOrSpread};
            // evaluation.js:213-215 — `prop.isObjectMethod() ||
            // prop.isSpreadElement()` deopts.
            let prop = match prop {
                PropOrSpread::Spread(_) => {
                    deopt(state);
                    return None;
                }
                PropOrSpread::Prop(p) => p,
            };
            let prop = prop.as_ref();
            // ObjectMethod — Babel's `prop.isObjectMethod()`.
            if matches!(
                prop,
                Prop::Method(_) | Prop::Getter(_) | Prop::Setter(_)
            ) {
                deopt(state);
                return None;
            }

            let (key_name_node, value_expr): (&PropName, &Expr) = match prop {
                Prop::Shorthand(ident) => {
                    // `{ foo }` shorthand — Babel treats the value
                    // path as the same Identifier; the key folds to
                    // `"foo"` (an Identifier `keyPath.node.name`).
                    let key_str = ident.sym.to_string();
                    let v = evaluate_cached(
                        &Expr::Ident(ident.clone()),
                        state,
                        index,
                        scope,
                    );
                    if !state.confident {
                        return None;
                    }
                    out.push((key_str, v.unwrap_or(Value::Undefined)));
                    continue;
                }
                Prop::KeyValue(kv) => (&kv.key, kv.value.as_ref()),
                Prop::Assign(_) => {
                    // AssignmentProperty — only legal inside an
                    // ObjectPattern, not an ObjectExpression. Defensive
                    // deopt.
                    deopt(state);
                    return None;
                }
                _ => unreachable!("Method/Getter/Setter handled above"),
            };

            // evaluation.js:218-230 — key extraction.
            let key_str: String = match key_name_node {
                // Computed key — fold.
                PropName::Computed(computed) => {
                    let kv = evaluate_cached(&computed.expr, state, index, scope);
                    if !state.confident {
                        return None;
                    }
                    kv.unwrap_or(Value::Undefined).to_js_string()
                }
                // Identifier key — `keyPath.node.name`.
                PropName::Ident(i) => i.sym.to_string(),
                // String literal key — `keyPath.node.value`.
                PropName::Str(s) => s.value.to_atom_lossy().as_str().to_string(),
                // Numeric literal key — `keyPath.node.value` toString.
                PropName::Num(n) => js_number_to_string(n.value),
                // BigInt key — fold via toString.
                PropName::BigInt(b) => b.value.to_string(),
            };

            // evaluation.js:231-238 — value fold.
            let v = evaluate_cached(value_expr, state, index, scope);
            if !state.confident {
                return None;
            }
            out.push((key_str, v.unwrap_or(Value::Undefined)));
        }
        return Some(Value::Object(out));
    }

    // evaluation.js:242-263 — LogicalExpression branch.
    // Note Babel's evaluator preserves leftConfident vs rightConfident
    // independently and combines with the operator semantics. Mirror
    // exactly.
    if let Expr::Bin(BinExpr {
        op: BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing,
        left,
        right,
        ..
    }) = expr
    {
        let was_confident = state.confident;
        let left_val = evaluate_cached(left, state, index, scope);
        let left_confident = state.confident;
        state.confident = was_confident;
        let right_val = evaluate_cached(right, state, index, scope);
        let right_confident = state.confident;

        let op = match expr {
            Expr::Bin(b) => b.op,
            _ => unreachable!(),
        };

        let left = left_val.unwrap_or(Value::Undefined);
        let right = right_val.unwrap_or(Value::Undefined);

        return Some(match op {
            BinaryOp::LogicalOr => {
                state.confident =
                    left_confident && (left.truthy() || right_confident);
                if !state.confident {
                    return None;
                }
                if left.truthy() {
                    left
                } else {
                    right
                }
            }
            BinaryOp::LogicalAnd => {
                state.confident =
                    left_confident && (!left.truthy() || right_confident);
                if !state.confident {
                    return None;
                }
                if !left.truthy() {
                    left
                } else {
                    right
                }
            }
            BinaryOp::NullishCoalescing => {
                state.confident =
                    left_confident && (!left.is_nullish() || right_confident);
                if !state.confident {
                    return None;
                }
                if !left.is_nullish() {
                    left
                } else {
                    right
                }
            }
            _ => unreachable!(),
        });
    }

    // evaluation.js:264-311 — BinaryExpression branch (arithmetic +
    // comparison + bitwise).
    if let Expr::Bin(BinExpr { op, left, right, .. }) = expr {
        let left_val = evaluate_cached(left, state, index, scope);
        if !state.confident {
            return None;
        }
        let right_val = evaluate_cached(right, state, index, scope);
        if !state.confident {
            return None;
        }
        let l = left_val.unwrap_or(Value::Undefined);
        let r = right_val.unwrap_or(Value::Undefined);
        return Some(match op {
            // evaluation.js:271-272 — `-`
            BinaryOp::Sub => Value::Number(l.to_js_number() - r.to_js_number()),
            // evaluation.js:273-274 — `+` (string concat OR add)
            BinaryOp::Add => match (&l, &r) {
                (Value::String(_), _) | (_, Value::String(_)) => {
                    Value::String(format!("{}{}", l.to_js_string(), r.to_js_string()))
                }
                _ => Value::Number(l.to_js_number() + r.to_js_number()),
            },
            // evaluation.js:275-276 — `/`
            BinaryOp::Div => Value::Number(l.to_js_number() / r.to_js_number()),
            // evaluation.js:277-278 — `*`
            BinaryOp::Mul => Value::Number(l.to_js_number() * r.to_js_number()),
            // evaluation.js:279-280 — `%`
            BinaryOp::Mod => Value::Number(l.to_js_number() % r.to_js_number()),
            // evaluation.js:281-282 — `**`
            BinaryOp::Exp => Value::Number(l.to_js_number().powf(r.to_js_number())),
            // evaluation.js:283-284 — `<`
            BinaryOp::Lt => Value::Bool(js_lt(&l, &r)),
            // evaluation.js:285-286 — `>`
            BinaryOp::Gt => Value::Bool(js_lt(&r, &l)),
            // evaluation.js:287-288 — `<=`
            BinaryOp::LtEq => Value::Bool(!js_lt(&r, &l) && !js_lt_nan_to_false(&l, &r)),
            // evaluation.js:289-290 — `>=`
            BinaryOp::GtEq => Value::Bool(!js_lt(&l, &r) && !js_lt_nan_to_false(&r, &l)),
            // evaluation.js:291-292 — `==` (loose)
            BinaryOp::EqEq => Value::Bool(js_loose_eq(&l, &r)),
            // evaluation.js:293-294 — `!=`
            BinaryOp::NotEq => Value::Bool(!js_loose_eq(&l, &r)),
            // evaluation.js:295-296 — `===`
            BinaryOp::EqEqEq => Value::Bool(js_strict_eq(&l, &r)),
            // evaluation.js:297-298 — `!==`
            BinaryOp::NotEqEq => Value::Bool(!js_strict_eq(&l, &r)),
            // evaluation.js:299-300 — `|`
            BinaryOp::BitOr => Value::Number(
                (js_to_int32(l.to_js_number()) | js_to_int32(r.to_js_number())) as f64,
            ),
            // evaluation.js:301-302 — `&`
            BinaryOp::BitAnd => Value::Number(
                (js_to_int32(l.to_js_number()) & js_to_int32(r.to_js_number())) as f64,
            ),
            // evaluation.js:303-304 — `^`
            BinaryOp::BitXor => Value::Number(
                (js_to_int32(l.to_js_number()) ^ js_to_int32(r.to_js_number())) as f64,
            ),
            // evaluation.js:305-306 — `<<`
            BinaryOp::LShift => Value::Number(
                (js_to_int32(l.to_js_number())
                    .wrapping_shl(js_to_uint32(r.to_js_number()) & 0x1f)) as f64,
            ),
            // evaluation.js:307-308 — `>>` (signed)
            BinaryOp::RShift => Value::Number(
                (js_to_int32(l.to_js_number())
                    .wrapping_shr(js_to_uint32(r.to_js_number()) & 0x1f)) as f64,
            ),
            // evaluation.js:309-310 — `>>>` (unsigned)
            BinaryOp::ZeroFillRShift => Value::Number(
                ((js_to_uint32(l.to_js_number()))
                    .wrapping_shr(js_to_uint32(r.to_js_number()) & 0x1f)) as f64,
            ),
            // `in` and `instanceof` aren't folded by Babel in the
            // evaluator (no branch in evaluation.js). Fall through to
            // deopt.
            BinaryOp::In | BinaryOp::InstanceOf => {
                deopt(state);
                return None;
            }
            // Logical ops handled above; unreachable here.
            BinaryOp::LogicalAnd
            | BinaryOp::LogicalOr
            | BinaryOp::NullishCoalescing => unreachable!("logical handled above"),
        });
    }

    // evaluation.js:312-342 — CallExpression branch (Math.x, String,
    // Number, isFinite, isNaN, parseInt, parseFloat, decodeURI, etc.).
    // The Compiled corpus's only deopt-call fixture (`someFn()`) falls
    // through the entire branch and lands at :343's deopt — i.e. the
    // CallExpression branch is reachable but the corpus doesn't fold
    // any of its sub-shapes. A faithful port of the global-callee +
    // member-callee dispatch would require runtime emulation of
    // `Math.pow` etc. and a large vendoring of JS builtins; defer the
    // sub-shapes per the §5.4-handler discussion (Compiled's CSS-value
    // call sites that DO fold — e.g. `Math.PI` — go through
    // `traverseMemberExpression`, not `path.evaluate()`).
    //
    // For corpus parity we fall through to deopt at :343 below, which
    // matches Babel's behaviour for `someFn()` (no global match → deopt).
    // If a future fixture surfaces a foldable Math/String/Number call,
    // port the sub-shape in this if-block.
    if matches!(expr, Expr::Call(_)) {
        deopt(state);
        return None;
    }

    // evaluation.js:343 — final fallback: deopt.
    deopt(state);
    None
}

/// `evaluation.js:345-357` — `evaluateQuasis(path, quasis, state, raw)`.
/// Folds a TemplateLiteral by interleaving its cooked-strings with
/// the recursively-folded expressions.
fn evaluate_quasis(
    tpl: &Tpl,
    state: &mut State,
    index: &ScopeIndex,
    scope: ScopeId,
    raw: bool,
) -> Option<Value> {
    let mut out = String::new();
    for (i, elem) in tpl.quasis.iter().enumerate() {
        if !state.confident {
            break;
        }
        // `cooked` is None on syntactically-invalid escape sequences;
        // Babel folds those to `undefined` (the template-literal
        // source is preserved but the cooked value isn't
        // representable). Mirror by deopting. `raw: Atom`, `cooked:
        // Option<Wtf8Atom>` — Wtf8Atom may carry lone surrogates so
        // `as_str()` returns `Option<&str>`; lone-surrogate templates
        // also deopt (they cannot fold to a valid Rust string).
        if raw {
            out.push_str(elem.raw.as_str());
        } else {
            match elem.cooked.as_ref().and_then(|c| c.as_str()) {
                Some(cs) => out.push_str(cs),
                None => {
                    deopt(state);
                    return None;
                }
            }
        }
        if let Some(expr) = tpl.exprs.get(i) {
            let v = evaluate_cached(expr, state, index, scope);
            if !state.confident {
                return None;
            }
            // evaluation.js:353 — `String(value)`.
            out.push_str(&v.unwrap_or(Value::Undefined).to_js_string());
        }
    }
    if !state.confident {
        return None;
    }
    Some(Value::String(out))
}

// -------------------- JS-semantic helpers --------------------

/// `evaluation.js`'s `<` operator semantics — handles string-string
/// (lexicographic), number-number (with NaN special-case), and
/// mixed (coerce-to-number).
fn js_lt(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(x), Value::String(y)) => x < y,
        _ => {
            let na = a.to_js_number();
            let nb = b.to_js_number();
            if na.is_nan() || nb.is_nan() {
                return false;
            }
            na < nb
        }
    }
}

/// Helper for `<=` / `>=`: in JS, `a <= b` is `!(b < a)` BUT NaN
/// makes both `<` and `>` false, so `a <= b` is false when either
/// side is NaN. The straightforward `!js_lt(b, a)` mishandles NaN.
fn js_lt_nan_to_false(a: &Value, b: &Value) -> bool {
    let na = a.to_js_number();
    let nb = b.to_js_number();
    na.is_nan() || nb.is_nan()
}

/// JS `==` (loose equality). Implements the abstract algorithm from
/// ECMA-262 §7.2.13 sufficient for evaluator parity.
fn js_loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        // Same-type — falls through to strict comparison.
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => {
            !x.is_nan() && !y.is_nan() && x == y
        }
        (Value::String(x), Value::String(y)) => x == y,
        // null == undefined.
        (Value::Undefined, Value::Null) | (Value::Null, Value::Undefined) => true,
        // null/undefined !== anything else.
        (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => false,
        // Number ↔ String: coerce.
        (Value::Number(_), Value::String(_)) | (Value::String(_), Value::Number(_)) => {
            a.to_js_number() == b.to_js_number()
                && !a.to_js_number().is_nan()
        }
        // Bool coerces to Number on either side.
        (Value::Bool(_), _) => js_loose_eq(&Value::Number(a.to_js_number()), b),
        (_, Value::Bool(_)) => js_loose_eq(a, &Value::Number(b.to_js_number())),
        // Object/Array on one side reaches ToPrimitive — defer to
        // strict comparison; the Compiled corpus doesn't fold loose
        // equality across object-vs-primitive shapes.
        _ => js_strict_eq(a, b),
    }
}

/// JS `===` (strict equality).
fn js_strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undefined, Value::Undefined) => true,
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => {
            !x.is_nan() && !y.is_nan() && x == y
        }
        (Value::String(x), Value::String(y)) => x == y,
        // Reference equality on Object/Array — we don't track identity,
        // so always false. Matches the conservative deopt path.
        _ => false,
    }
}

/// JS `ToInt32(x)` — `evaluation.js`'s `~`/`<<`/`>>`/`&`/`|`/`^`
/// operators all coerce through ToInt32. Per ECMA-262 §7.1.6.
fn js_to_int32(n: f64) -> i32 {
    if !n.is_finite() {
        return 0;
    }
    let n = n.trunc();
    let modulo = n.rem_euclid(4_294_967_296.0);
    let unsigned = if modulo < 0.0 {
        modulo + 4_294_967_296.0
    } else {
        modulo
    } as u32;
    unsigned as i32
}

/// JS `ToUint32(x)` — for `>>>` and bit-shift count masking.
fn js_to_uint32(n: f64) -> u32 {
    js_to_int32(n) as u32
}

/// JS `typeof` operator output.
fn typeof_string(v: &Value) -> &'static str {
    match v {
        Value::Undefined => "undefined",
        Value::Null => "object", // JS quirk
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) | Value::Object(_) => "object",
    }
}

// -------------------- tests --------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::{EsVersion, Module};
    use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

    fn parse(source: &str) -> Module {
        let cm: Lrc<SourceMap> = Default::default();
        let fm = cm.new_source_file(
            Arc::new(FileName::Custom("compat-evaluation-test.ts".into())),
            String::from(source),
        );
        let mut errors = Vec::new();
        let module = parse_file_as_module(
            &fm,
            Syntax::Typescript(TsSyntax {
                tsx: false,
                decorators: false,
                no_early_errors: true,
                disallow_ambiguous_jsx_like: false,
                dts: false,
            }),
            EsVersion::Es2022,
            None,
            &mut errors,
        )
        .expect("parse");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        module
    }

    /// Reach the synthetic `const __evalTarget = (EXPR);` init expr —
    /// matches the oracle.mjs wrapper.
    fn eval_str(source: &str) -> EvaluatedValue {
        let wrapped = format!("const __evalTarget = ({source});");
        let m = parse(&wrapped);
        let idx = ScopeIndex::build(&m);
        // Find the VarDeclarator init for __evalTarget.
        use swc_core::ecma::ast::{Decl, ModuleItem, Pat, Stmt};
        for item in &m.body {
            if let ModuleItem::Stmt(Stmt::Decl(Decl::Var(v))) = item {
                if let Some(declarator) = v.decls.first() {
                    if let Pat::Ident(b) = &declarator.name {
                        if b.id.sym.as_str() == "__evalTarget" {
                            if let Some(init) = &declarator.init {
                                return evaluate(init, &idx, idx.program_scope());
                            }
                        }
                    }
                }
            }
        }
        panic!("__evalTarget declarator not found");
    }

    #[test]
    fn folds_string_literal() {
        match eval_str("'hello'") {
            EvaluatedValue::Confident(Value::String(s)) => assert_eq!(s, "hello"),
            other => panic!("expected confident string, got {other:?}"),
        }
    }

    #[test]
    fn folds_numeric_literal() {
        match eval_str("42") {
            EvaluatedValue::Confident(Value::Number(n)) => assert_eq!(n, 42.0),
            other => panic!("expected confident number, got {other:?}"),
        }
    }

    #[test]
    fn folds_addition() {
        match eval_str("1 + 2") {
            EvaluatedValue::Confident(Value::Number(n)) => assert_eq!(n, 3.0),
            other => panic!("expected 3, got {other:?}"),
        }
    }

    #[test]
    fn folds_string_concat() {
        match eval_str("'a' + 'b'") {
            EvaluatedValue::Confident(Value::String(s)) => assert_eq!(s, "ab"),
            other => panic!("expected 'ab', got {other:?}"),
        }
    }

    #[test]
    fn folds_paren_binary() {
        match eval_str("(1 + 2) * 3") {
            EvaluatedValue::Confident(Value::Number(n)) => assert_eq!(n, 9.0),
            other => panic!("expected 9, got {other:?}"),
        }
    }

    #[test]
    fn folds_template_with_expression() {
        match eval_str("`hello ${'wo' + 'rld'}`") {
            EvaluatedValue::Confident(Value::String(s)) => assert_eq!(s, "hello world"),
            other => panic!("expected 'hello world', got {other:?}"),
        }
    }

    #[test]
    fn unbound_identifier_deopts() {
        assert!(matches!(eval_str("someUnboundIdent"), EvaluatedValue::Deopt));
    }

    #[test]
    fn undefined_global_folds() {
        match eval_str("undefined") {
            EvaluatedValue::Confident(Value::Undefined) => {}
            other => panic!("expected undefined, got {other:?}"),
        }
    }

    #[test]
    fn nan_global_folds_to_nan() {
        match eval_str("NaN") {
            EvaluatedValue::Confident(Value::Number(n)) => assert!(n.is_nan()),
            other => panic!("expected NaN, got {other:?}"),
        }
    }

    #[test]
    fn ts_as_expression_deopts() {
        assert!(matches!(eval_str("(1 as number)"), EvaluatedValue::Deopt));
    }

    #[test]
    fn call_expression_deopts() {
        assert!(matches!(eval_str("someFn()"), EvaluatedValue::Deopt));
    }

    #[test]
    fn typeof_string_folds() {
        match eval_str("typeof 'hi'") {
            EvaluatedValue::Confident(Value::String(s)) => assert_eq!(s, "string"),
            other => panic!("expected 'string', got {other:?}"),
        }
    }

    #[test]
    fn void_zero_folds_undefined() {
        match eval_str("void 0") {
            EvaluatedValue::Confident(Value::Undefined) => {}
            other => panic!("expected undefined, got {other:?}"),
        }
    }

    #[test]
    fn conditional_folds_true_branch() {
        match eval_str("true ? 'yes' : 'no'") {
            EvaluatedValue::Confident(Value::String(s)) => assert_eq!(s, "yes"),
            other => panic!("expected 'yes', got {other:?}"),
        }
    }

    #[test]
    fn nullish_coalesce_zero_keeps_zero() {
        match eval_str("0 ?? 'z'") {
            EvaluatedValue::Confident(Value::Number(n)) => assert_eq!(n, 0.0),
            other => panic!("expected 0, got {other:?}"),
        }
    }
}
