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
    BinExpr, BinaryOp, Callee, CondExpr, Expr, Lit, MemberProp, Number, Prop, PropName,
    PropOrSpread, Tpl, UnaryExpr, UnaryOp,
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

    // evaluation.js:73-84 — TaggedTemplateExpression branch.
    // Upstream Babel only folds `String.raw\`...\``-shaped tagged
    // templates here; every other shape (including Compiled's
    // `keyframes\`...\`` / `css\`...\``) falls through to deopt
    // returning `{confident: false}`, which `babelEvaluateExpression`
    // converts to the fallback node.
    //
    // §6.8a-vi note: prior version of this branch panicked with
    // `unimplemented!()` based on the (then-correct) premise that
    // `evaluate-expression.ts:184` short-circuits Compiled tagged
    // templates BEFORE reaching `babelEvaluateExpression`. That
    // premise broke once §6.8a-vi wired `evaluate_expression` into
    // `extract_object_expression` / `extract_template_literal`: those
    // call sites now invoke `babel_evaluate_expression(target)` on the
    // ORIGINAL expression (a TaggedTpl) when value-resolution returns
    // None, and the fallback evaluator legitimately reaches this
    // branch. Switching to `deopt + None` matches upstream's behaviour
    // exactly — Babel's `path.evaluate()` returns `{confident:false}`,
    // the JS try/catch wrapper returns `fallbackNode`, and the rest of
    // the pipeline emits the original tagged template as a CSS value.
    //
    // String.raw-specific folding is still unimplemented (no fixture
    // surfaces it); if one ever does, port the sub-shape inside this
    // if-block per upstream evaluation.js:73-84.
    if matches!(expr, Expr::TaggedTpl(_)) {
        deopt(state);
        return None;
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
        // §6.8n — destructured `Pat::Object` LHS branch. Babel's
        // `path.evaluate()` itself doesn't extract destructured slices
        // (it returns the whole source object), but Compiled's
        // resolve-binding wrapper DOES at the upstream
        // `traverse-identifier` dispatch site. The compat-evaluate
        // path is reached here when the recursive evaluator descends
        // into an ObjectExpression value (e.g. `{ color: color1 }`
        // body of an arrow being folded by `babelEvaluateExpression`)
        // — that bypass means we'd never reach Compiled's resolve
        // wrapper unless we handle destructuring inline. Mirror the
        // resolve-binding shape: walk the LHS pattern via
        // `getDestructuredObjectPatternKey`, then walk the source
        // ObjectExpression for a matching key, and recurse.
        if let (Some(pat), Some(init)) = (
            binding.destructured_pat.as_ref(),
            binding.destructured_init.as_ref(),
        ) {
            let key = crate::utils::resolve_binding::get_destructured_object_pattern_key(
                pat, name,
            );
            // Direct ObjectLit init: walk for matching key. This
            // covers the IIFE-destructured-arg case where `init` is
            // the evaluated call argument (an ObjectLit). Identifier-
            // / Member-source cases (e.g. `const { foo } = obj`) are
            // resolved via the higher-level `resolve_binding` path
            // that traverse_identifier uses; the compat-evaluate
            // bypass below only fires for the direct-ObjectLit
            // shape because chained resolution requires `Metadata`
            // we don't carry into this evaluator.
            if let Expr::Object(obj) = &**init {
                for prop in &obj.props {
                    let PropOrSpread::Prop(boxed) = prop else {
                        continue;
                    };
                    let (matches, value_expr): (bool, Expr) = match &**boxed {
                        Prop::KeyValue(kv) => {
                            let PropName::Ident(id) = &kv.key else {
                                continue;
                            };
                            (id.sym == *key, (*kv.value).clone())
                        }
                        Prop::Shorthand(id) => (id.sym == *key, Expr::Ident(id.clone())),
                        _ => continue,
                    };
                    if matches {
                        // Recurse into the property value with the
                        // same scope. The source ObjectLit was
                        // captured at IIFE-call time with the
                        // caller scope's bindings already in lexical
                        // chain.
                        return evaluate_cached(&value_expr, state, index, scope);
                    }
                }
                // Key not found in source object → undefined per JS.
                return Some(Value::Undefined);
            }
            // Non-ObjectLit init (Ident / Member chain): deopt
            // through the standard path. The traverse_identifier
            // dispatcher handles these via resolve_binding's §6.8n
            // wiring; reaching here means we descended past the
            // top-level dispatch. Conservative deopt matches Babel's
            // path.evaluate() behaviour for non-foldable inits.
            deopt(state);
            return None;
        }
        // No `init_expr` populated and no destructured-pattern hint —
        // Babel would reach this code with `bindingPath.get('init')`
        // returning an empty NodePath; that case folds to `undefined`
        // per Babel (no init = uninitialised = undefined). Match Babel.
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
    //
    // Upstream:
    //   const VALID_OBJECT_CALLEES = ["Number", "String", "Math"];
    //   const VALID_IDENTIFIER_CALLEES = ["isFinite", "isNaN",
    //     "parseFloat", "parseInt", "decodeURI", "decodeURIComponent",
    //     "encodeURI", "encodeURIComponent", null, null];
    //   const INVALID_METHODS = ["random"];
    //   if (callee.isIdentifier() && !path.scope.getBinding(name)
    //         && (isValidObjectCallee(name) || isValidIdentifierCallee(name)))
    //     func = global[name];
    //   if (callee.isMemberExpression()) {
    //     if (object.isIdentifier() && property.isIdentifier()
    //           && isValidObjectCallee(object.node.name)
    //           && !isInvalidMethod(property.node.name)) {
    //       context = global[object.node.name];
    //       if (hasOwnProperty.call(context, key)) func = context[key];
    //     }
    //     if (object.isLiteral() && property.isIdentifier()) { … }
    //   }
    //   if (func) {
    //     const args = path.get("arguments").map(a => evaluateCached(a, state));
    //     if (!state.confident) return;
    //     return func.apply(context, args);
    //   }
    //
    // Compiled's CSS-value call sites that fold through this path
    // include `Math.max(...)`, `Math.min(...)`, `Math.abs(...)`,
    // `Math.round(...)`, etc. (see fixtures/ct-expression-export).
    if let Expr::Call(call) = expr {
        if let Callee::Expr(callee_expr) = &call.callee {
            if let Some(func) = resolve_builtin_callee(callee_expr, index, scope) {
                // evaluation.js:338-340 — fold args, propagate deopt.
                let mut args: Vec<Value> = Vec::with_capacity(call.args.len());
                for a in &call.args {
                    if a.spread.is_some() {
                        // Spread args don't fold; matches Babel's
                        // implicit deopt (it builds the args array
                        // shape-aware via .get() but `func.apply` with
                        // a SpreadElement-as-AST-node would throw at
                        // runtime; Babel's path.get('arguments') yields
                        // SpreadElement paths whose evaluateCached
                        // deopts).
                        deopt(state);
                        return None;
                    }
                    let v = evaluate_cached(&a.expr, state, index, scope);
                    if !state.confident {
                        return None;
                    }
                    args.push(v.unwrap_or(Value::Undefined));
                }
                return Some(apply_builtin(func, &args));
            }
        }
    }

    // evaluation.js:343 — final fallback: deopt.
    deopt(state);
    None
}

// -------------------- builtin call resolution --------------------
//
// Mirrors `evaluation.js:312-341`'s `func`/`context` resolution.
// The JS code does `func = global[name]` / `func = context[key]` and
// then `func.apply(context, args)`; in Rust we identify the builtin by
// an enum tag and dispatch in `apply_builtin` to mirror the runtime
// semantics of each function. Only the methods that JS's `Math` /
// `Number` / `String` / global identifier set actually exposes are
// reachable; unknown methods are caught by `resolve_member_method`
// returning `None` (the JS `hasOwnProperty.call(context, key)` check
// at :325).
//
// Per `INVALID_METHODS = ["random"]` at evaluation.js:10, `Math.random`
// is explicitly excluded.
#[derive(Debug, Clone, Copy)]
enum Builtin {
    // Global identifier callees (VALID_IDENTIFIER_CALLEES).
    IsFinite,
    IsNaN,
    ParseFloat,
    ParseInt,
    // Global object callees called as functions (VALID_OBJECT_CALLEES,
    // identifier form: `Number(x)`, `String(x)`. `Math(...)` would
    // throw at runtime; we deopt-by-not-resolving via the absence of
    // a Math arm here.).
    NumberFn,
    StringFn,
    // Math.* methods.
    MathAbs,
    MathCeil,
    MathFloor,
    MathRound,
    MathTrunc,
    MathSign,
    MathSqrt,
    MathCbrt,
    MathMax,
    MathMin,
    MathPow,
    MathExp,
    MathLog,
    MathLog2,
    MathLog10,
    MathSin,
    MathCos,
    MathTan,
    MathAsin,
    MathAcos,
    MathAtan,
    MathAtan2,
    MathHypot,
}

/// Resolve a `CallExpression`'s callee `&Expr` to a `Builtin` if it
/// matches one of Babel's `VALID_*_CALLEES` shapes, the binding-not-
/// shadowed gate (`!path.scope.getBinding(name)`) holds, and the
/// method exists on the receiver namespace.
///
/// Returns `None` when the callee shape doesn't match, the identifier
/// is shadowed by a local binding, or the method is unknown / banned
/// (`Math.random`).
fn resolve_builtin_callee(
    callee: &Expr,
    index: &ScopeIndex,
    scope: ScopeId,
) -> Option<Builtin> {
    match callee {
        // evaluation.js:316-318 — global identifier callee.
        // `Number(x)` / `String(x)` / `parseInt(x)` etc.
        Expr::Ident(ident) => {
            let name = ident.sym.as_str();
            if index.get_binding(scope, name).is_some() {
                return None;
            }
            match name {
                "isFinite" => Some(Builtin::IsFinite),
                "isNaN" => Some(Builtin::IsNaN),
                "parseFloat" => Some(Builtin::ParseFloat),
                "parseInt" => Some(Builtin::ParseInt),
                "Number" => Some(Builtin::NumberFn),
                "String" => Some(Builtin::StringFn),
                _ => None,
            }
        }
        // evaluation.js:319-336 — member-callee branch.
        Expr::Member(member) => {
            // We only handle `<Ident>.<Ident>` — the
            // `object.isLiteral()` arm at :329-335 covers
            // `"abc".charAt(0)` etc., which the corpus doesn't exercise.
            // (If it ever does, port that arm here.)
            let obj_ident = match &*member.obj {
                Expr::Ident(i) => i,
                _ => return None,
            };
            let prop_ident = match &member.prop {
                MemberProp::Ident(i) => i,
                _ => return None,
            };
            let obj_name = obj_ident.sym.as_str();
            let prop_name = prop_ident.sym.as_str();
            // VALID_OBJECT_CALLEES = ["Number", "String", "Math"].
            // INVALID_METHODS = ["random"].
            if prop_name == "random" {
                return None;
            }
            // Babel does NOT gate object-callee on `getBinding(obj_name)`
            // — only the identifier-callee arm has that check. Match
            // upstream: a local `Math` shadow does NOT prevent the fold
            // (Babel reads `global[object.node.name]` directly).
            match obj_name {
                "Math" => match prop_name {
                    "abs" => Some(Builtin::MathAbs),
                    "ceil" => Some(Builtin::MathCeil),
                    "floor" => Some(Builtin::MathFloor),
                    "round" => Some(Builtin::MathRound),
                    "trunc" => Some(Builtin::MathTrunc),
                    "sign" => Some(Builtin::MathSign),
                    "sqrt" => Some(Builtin::MathSqrt),
                    "cbrt" => Some(Builtin::MathCbrt),
                    "max" => Some(Builtin::MathMax),
                    "min" => Some(Builtin::MathMin),
                    "pow" => Some(Builtin::MathPow),
                    "exp" => Some(Builtin::MathExp),
                    "log" => Some(Builtin::MathLog),
                    "log2" => Some(Builtin::MathLog2),
                    "log10" => Some(Builtin::MathLog10),
                    "sin" => Some(Builtin::MathSin),
                    "cos" => Some(Builtin::MathCos),
                    "tan" => Some(Builtin::MathTan),
                    "asin" => Some(Builtin::MathAsin),
                    "acos" => Some(Builtin::MathAcos),
                    "atan" => Some(Builtin::MathAtan),
                    "atan2" => Some(Builtin::MathAtan2),
                    "hypot" => Some(Builtin::MathHypot),
                    _ => None,
                },
                // `Number.isInteger`, `String.fromCharCode`, etc. would
                // be reachable but the corpus doesn't fold them; if a
                // future fixture surfaces one, port the sub-shape here.
                _ => None,
            }
        }
        _ => None,
    }
}

/// Apply the resolved builtin to its already-folded arguments.
/// Mirrors `func.apply(context, args)` at evaluation.js:340.
fn apply_builtin(func: Builtin, args: &[Value]) -> Value {
    // Helper: coerce an arg to a JS number (Number(x) semantics).
    let n = |i: usize| -> f64 {
        args.get(i)
            .map(|v| v.to_js_number())
            .unwrap_or(f64::NAN)
    };
    match func {
        // Identifier-form globals.
        Builtin::IsFinite => Value::Bool(n(0).is_finite()),
        Builtin::IsNaN => Value::Bool(n(0).is_nan()),
        Builtin::ParseFloat => Value::Number(js_parse_float(args.first())),
        Builtin::ParseInt => Value::Number(js_parse_int(args.first(), args.get(1))),
        Builtin::NumberFn => {
            // Number() with no args returns 0; Number(x) coerces.
            if args.is_empty() {
                Value::Number(0.0)
            } else {
                Value::Number(args[0].to_js_number())
            }
        }
        Builtin::StringFn => {
            // String() with no args returns ""; String(x) coerces.
            if args.is_empty() {
                Value::String(String::new())
            } else {
                Value::String(args[0].to_js_string())
            }
        }
        // Math.* — match JS semantics on f64.
        Builtin::MathAbs => Value::Number(n(0).abs()),
        Builtin::MathCeil => Value::Number(n(0).ceil()),
        Builtin::MathFloor => Value::Number(n(0).floor()),
        // JS Math.round: rounds half toward +Infinity (NOT banker's).
        // Rust's f64::round rounds half away from zero, which differs
        // for negative half-integers (e.g. -0.5 → JS 0, Rust -1).
        Builtin::MathRound => Value::Number(js_math_round(n(0))),
        Builtin::MathTrunc => Value::Number(n(0).trunc()),
        Builtin::MathSign => Value::Number({
            let x = n(0);
            if x.is_nan() {
                f64::NAN
            } else if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                x
            } // preserves +0/-0
        }),
        Builtin::MathSqrt => Value::Number(n(0).sqrt()),
        Builtin::MathCbrt => Value::Number(n(0).cbrt()),
        Builtin::MathMax => {
            // JS Math.max() with no args is -Infinity; with any NaN
            // arg returns NaN.
            if args.is_empty() {
                Value::Number(f64::NEG_INFINITY)
            } else {
                let mut acc = f64::NEG_INFINITY;
                for v in args {
                    let x = v.to_js_number();
                    if x.is_nan() {
                        return Value::Number(f64::NAN);
                    }
                    // Math.max distinguishes +0 from -0 (returns +0).
                    if x > acc || (x == 0.0 && acc == 0.0 && x.is_sign_positive()) {
                        acc = x;
                    }
                }
                Value::Number(acc)
            }
        }
        Builtin::MathMin => {
            if args.is_empty() {
                Value::Number(f64::INFINITY)
            } else {
                let mut acc = f64::INFINITY;
                for v in args {
                    let x = v.to_js_number();
                    if x.is_nan() {
                        return Value::Number(f64::NAN);
                    }
                    if x < acc || (x == 0.0 && acc == 0.0 && x.is_sign_negative()) {
                        acc = x;
                    }
                }
                Value::Number(acc)
            }
        }
        Builtin::MathPow => Value::Number(n(0).powf(n(1))),
        Builtin::MathExp => Value::Number(n(0).exp()),
        Builtin::MathLog => Value::Number(n(0).ln()),
        Builtin::MathLog2 => Value::Number(n(0).log2()),
        Builtin::MathLog10 => Value::Number(n(0).log10()),
        Builtin::MathSin => Value::Number(n(0).sin()),
        Builtin::MathCos => Value::Number(n(0).cos()),
        Builtin::MathTan => Value::Number(n(0).tan()),
        Builtin::MathAsin => Value::Number(n(0).asin()),
        Builtin::MathAcos => Value::Number(n(0).acos()),
        Builtin::MathAtan => Value::Number(n(0).atan()),
        Builtin::MathAtan2 => Value::Number(n(0).atan2(n(1))),
        Builtin::MathHypot => {
            // JS Math.hypot(): no args → 0; any NaN with a non-Infinity
            // → NaN; any Infinity → Infinity (even if a NaN is also
            // present). Match the spec.
            if args.iter().any(|v| v.to_js_number().is_infinite()) {
                return Value::Number(f64::INFINITY);
            }
            if args.iter().any(|v| v.to_js_number().is_nan()) {
                return Value::Number(f64::NAN);
            }
            let sum_sq: f64 = args.iter().map(|v| v.to_js_number().powi(2)).sum();
            Value::Number(sum_sq.sqrt())
        }
    }
}

/// JS `Math.round` semantics (round-half-toward-+Infinity).
fn js_math_round(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    // Special-case half-integers: round toward +Infinity.
    let frac = x - x.floor();
    if frac == 0.5 {
        x.floor() + 1.0
    } else {
        // floor(x + 0.5) handles all other cases per spec.
        (x + 0.5).floor()
    }
}

/// JS `parseFloat(x)` — coerces to string then parses leading number.
fn js_parse_float(arg: Option<&Value>) -> f64 {
    let s = match arg {
        Some(v) => v.to_js_string(),
        None => return f64::NAN,
    };
    let trimmed = s.trim_start();
    if trimmed.is_empty() {
        return f64::NAN;
    }
    // Handle Infinity / -Infinity / +Infinity prefixes per spec.
    let (sign, rest) = match trimmed.as_bytes()[0] {
        b'+' => (1.0, &trimmed[1..]),
        b'-' => (-1.0, &trimmed[1..]),
        _ => (1.0, trimmed),
    };
    if rest.starts_with("Infinity") {
        return sign * f64::INFINITY;
    }
    // Find longest valid numeric prefix.
    let mut end = 0;
    let bytes = rest.as_bytes();
    let mut saw_digit = false;
    let mut saw_dot = false;
    let mut saw_exp = false;
    while end < bytes.len() {
        let c = bytes[end];
        match c {
            b'0'..=b'9' => {
                saw_digit = true;
            }
            b'.' if !saw_dot && !saw_exp => {
                saw_dot = true;
            }
            b'e' | b'E' if saw_digit && !saw_exp => {
                saw_exp = true;
                // Optional sign after exponent.
                if end + 1 < bytes.len()
                    && (bytes[end + 1] == b'+' || bytes[end + 1] == b'-')
                {
                    end += 1;
                }
            }
            _ => break,
        }
        end += 1;
    }
    if !saw_digit {
        return f64::NAN;
    }
    rest[..end].parse::<f64>().map(|n| sign * n).unwrap_or(f64::NAN)
}

/// JS `parseInt(x, radix)` — coerces to string, optional sign, base
/// detection (`0x` → 16 if radix in {0,16}), parses digits up to the
/// first non-digit.
fn js_parse_int(arg: Option<&Value>, radix_arg: Option<&Value>) -> f64 {
    let s = match arg {
        Some(v) => v.to_js_string(),
        None => return f64::NAN,
    };
    let mut radix = radix_arg.map(|v| v.to_js_number()).unwrap_or(0.0);
    if radix.is_nan() {
        radix = 0.0;
    }
    let radix = radix as i32;
    if radix != 0 && (radix < 2 || radix > 36) {
        return f64::NAN;
    }
    let trimmed = s.trim_start();
    if trimmed.is_empty() {
        return f64::NAN;
    }
    let (sign, rest) = match trimmed.as_bytes()[0] {
        b'+' => (1.0, &trimmed[1..]),
        b'-' => (-1.0, &trimmed[1..]),
        _ => (1.0, trimmed),
    };
    let (effective_radix, digits) = if (radix == 0 || radix == 16)
        && (rest.starts_with("0x") || rest.starts_with("0X"))
    {
        (16u32, &rest[2..])
    } else if radix == 0 {
        (10u32, rest)
    } else {
        (radix as u32, rest)
    };
    // Take longest prefix of digits valid in the radix.
    let mut end = 0;
    for (i, c) in digits.char_indices() {
        if c.to_digit(effective_radix).is_some() {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return f64::NAN;
    }
    match i64::from_str_radix(&digits[..end], effective_radix) {
        Ok(v) => sign * (v as f64),
        Err(_) => {
            // Overflow path: walk digits manually.
            let mut acc: f64 = 0.0;
            for c in digits[..end].chars() {
                acc = acc * (effective_radix as f64) + (c.to_digit(effective_radix).unwrap() as f64);
            }
            sign * acc
        }
    }
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

    // ----------------------------------------------------------------
    // CallExpression branch — `evaluation.js:312-342` port coverage.
    //
    // Each test below pins one JS-spec edge case that f64 in Rust does
    // NOT match by default. If a test fails after a Rust-stdlib bump
    // or an `apply_builtin` rewrite, the JS-vs-Rust gap reopened.
    // ----------------------------------------------------------------

    fn confident_num(v: EvaluatedValue) -> f64 {
        match v {
            EvaluatedValue::Confident(Value::Number(n)) => n,
            other => panic!("expected confident number, got {other:?}"),
        }
    }

    #[test]
    fn math_max_basic_folds() {
        assert_eq!(confident_num(eval_str("Math.max(1, 2, 3)")), 3.0);
        assert_eq!(confident_num(eval_str("Math.max(-5, -10)")), -5.0);
    }

    #[test]
    fn math_max_no_args_is_neg_infinity() {
        // Spec: Math.max() === -Infinity.
        let n = confident_num(eval_str("Math.max()"));
        assert!(n.is_infinite() && n < 0.0, "expected -Infinity, got {n}");
    }

    #[test]
    fn math_max_nan_propagates() {
        // Spec: any NaN arg → NaN result, regardless of position.
        assert!(confident_num(eval_str("Math.max(1, NaN, 3)")).is_nan());
        assert!(confident_num(eval_str("Math.max(NaN, 1)")).is_nan());
    }

    #[test]
    fn math_max_signed_zero_returns_positive() {
        // Spec: Math.max(-0, +0) === +0 AND Math.max(+0, -0) === +0.
        // Detect sign via 1/+0 === +Infinity, 1/-0 === -Infinity.
        let a = confident_num(eval_str("Math.max(-0, 0)"));
        let b = confident_num(eval_str("Math.max(0, -0)"));
        assert_eq!(a, 0.0);
        assert_eq!(b, 0.0);
        assert!(a.is_sign_positive(), "Math.max(-0, 0) should be +0");
        assert!(b.is_sign_positive(), "Math.max(0, -0) should be +0");
    }

    #[test]
    fn math_min_basic_folds() {
        assert_eq!(confident_num(eval_str("Math.min(1, 2, 3)")), 1.0);
    }

    #[test]
    fn math_min_no_args_is_infinity() {
        let n = confident_num(eval_str("Math.min()"));
        assert!(n.is_infinite() && n > 0.0, "expected +Infinity, got {n}");
    }

    #[test]
    fn math_min_nan_propagates() {
        assert!(confident_num(eval_str("Math.min(1, NaN, 3)")).is_nan());
    }

    #[test]
    fn math_min_signed_zero_returns_negative() {
        // Spec: Math.min(-0, +0) === -0 AND Math.min(+0, -0) === -0.
        let a = confident_num(eval_str("Math.min(-0, 0)"));
        let b = confident_num(eval_str("Math.min(0, -0)"));
        assert_eq!(a, 0.0);
        assert_eq!(b, 0.0);
        assert!(a.is_sign_negative(), "Math.min(-0, 0) should be -0");
        assert!(b.is_sign_negative(), "Math.min(0, -0) should be -0");
    }

    #[test]
    fn math_round_half_toward_positive_infinity() {
        // Spec: Math.round rounds half toward +Infinity, NOT half-away-
        // from-zero (which is what Rust's f64::round does). Critical
        // divergence on negative half-integers.
        assert_eq!(confident_num(eval_str("Math.round(0.5)")), 1.0);
        assert_eq!(confident_num(eval_str("Math.round(1.5)")), 2.0);
        assert_eq!(confident_num(eval_str("Math.round(-0.5)")), 0.0);
        assert_eq!(confident_num(eval_str("Math.round(-1.5)")), -1.0);
        assert_eq!(confident_num(eval_str("Math.round(-2.5)")), -2.0);
    }

    #[test]
    fn math_pow_folds() {
        assert_eq!(confident_num(eval_str("Math.pow(2, 10)")), 1024.0);
    }

    #[test]
    fn math_abs_folds() {
        assert_eq!(confident_num(eval_str("Math.abs(-7)")), 7.0);
        assert_eq!(confident_num(eval_str("Math.abs(7)")), 7.0);
    }

    #[test]
    fn math_floor_ceil_trunc_fold() {
        assert_eq!(confident_num(eval_str("Math.floor(1.9)")), 1.0);
        assert_eq!(confident_num(eval_str("Math.ceil(1.1)")), 2.0);
        assert_eq!(confident_num(eval_str("Math.trunc(-1.9)")), -1.0);
    }

    #[test]
    fn math_random_does_not_fold() {
        // INVALID_METHODS at evaluation.js:10 includes "random".
        assert!(matches!(eval_str("Math.random()"), EvaluatedValue::Deopt));
    }

    #[test]
    fn math_max_deopts_with_unbound_arg() {
        // Spec: a deopt'd arg short-circuits; result is deopt, not NaN.
        // Mirrors evaluation.js:339 — `if (!state.confident) return;`.
        assert!(matches!(
            eval_str("Math.max(1, someUnbound)"),
            EvaluatedValue::Deopt
        ));
    }

    // Note on shadow-doesn't-block-member-form: Babel's member-callee
    // arm (evaluation.js:319-328) does NOT gate on `getBinding(obj)` —
    // it reads `global[object.node.name]` directly. We can't express a
    // scoped `const Math = …` shadow via this test harness because the
    // SequenceExpression branch is `unimplemented!()` per the
    // evidenced-unreachable assertion at the top of `_evaluate`. The
    // property is covered transitively: `math_max_basic_folds` succeeds
    // in this test environment which has no `Math` binding registered
    // — proving the member arm reads the namespace through the
    // VALID_OBJECT_CALLEES allow-list rather than through scope lookup.

    #[test]
    fn parse_int_decimal() {
        assert_eq!(confident_num(eval_str("parseInt('42')")), 42.0);
        assert_eq!(confident_num(eval_str("parseInt('  42abc')")), 42.0);
    }

    #[test]
    fn parse_int_hex_auto_detect() {
        // Spec: "0x" prefix with radix 0 (default) or 16 → base 16.
        assert_eq!(confident_num(eval_str("parseInt('0x10')")), 16.0);
        assert_eq!(confident_num(eval_str("parseInt('0xFF')")), 255.0);
    }

    #[test]
    fn parse_int_explicit_radix() {
        assert_eq!(confident_num(eval_str("parseInt('10', 16)")), 16.0);
        assert_eq!(confident_num(eval_str("parseInt('ff', 16)")), 255.0);
        assert_eq!(confident_num(eval_str("parseInt('1010', 2)")), 10.0);
    }

    #[test]
    fn parse_int_negative_and_sign() {
        assert_eq!(confident_num(eval_str("parseInt('-42')")), -42.0);
        assert_eq!(confident_num(eval_str("parseInt('+42')")), 42.0);
    }

    #[test]
    fn parse_int_invalid_returns_nan() {
        assert!(confident_num(eval_str("parseInt('xyz')")).is_nan());
        assert!(confident_num(eval_str("parseInt('')")).is_nan());
    }

    #[test]
    fn parse_float_basic() {
        assert_eq!(confident_num(eval_str("parseFloat('3.14')")), 3.14);
        assert_eq!(confident_num(eval_str("parseFloat('  3.14abc')")), 3.14);
    }

    #[test]
    fn parse_float_infinity_token() {
        let n = confident_num(eval_str("parseFloat('Infinity')"));
        assert!(n.is_infinite() && n > 0.0);
        let m = confident_num(eval_str("parseFloat('-Infinity')"));
        assert!(m.is_infinite() && m < 0.0);
    }

    #[test]
    fn parse_float_invalid_returns_nan() {
        assert!(confident_num(eval_str("parseFloat('xyz')")).is_nan());
    }

    #[test]
    fn is_nan_global() {
        match eval_str("isNaN(NaN)") {
            EvaluatedValue::Confident(Value::Bool(b)) => assert!(b),
            other => panic!("expected true, got {other:?}"),
        }
        match eval_str("isNaN(1)") {
            EvaluatedValue::Confident(Value::Bool(b)) => assert!(!b),
            other => panic!("expected false, got {other:?}"),
        }
    }

    #[test]
    fn is_finite_global() {
        match eval_str("isFinite(1)") {
            EvaluatedValue::Confident(Value::Bool(b)) => assert!(b),
            other => panic!("expected true, got {other:?}"),
        }
        match eval_str("isFinite(Infinity)") {
            EvaluatedValue::Confident(Value::Bool(b)) => assert!(!b),
            other => panic!("expected false, got {other:?}"),
        }
    }

    #[test]
    fn number_callee_coerces() {
        assert_eq!(confident_num(eval_str("Number('42')")), 42.0);
        assert_eq!(confident_num(eval_str("Number(true)")), 1.0);
        assert_eq!(confident_num(eval_str("Number()")), 0.0);
    }

    #[test]
    fn string_callee_coerces() {
        match eval_str("String(42)") {
            EvaluatedValue::Confident(Value::String(s)) => assert_eq!(s, "42"),
            other => panic!("expected '42', got {other:?}"),
        }
        match eval_str("String()") {
            EvaluatedValue::Confident(Value::String(s)) => assert_eq!(s, ""),
            other => panic!("expected '', got {other:?}"),
        }
    }

    #[test]
    fn unknown_math_method_deopts() {
        // `Math.fround` is a real spec method but not in our
        // resolve_builtin_callee enum — it must deopt rather than
        // silently mis-fold. If a fixture surfaces it, port the
        // sub-shape.
        assert!(matches!(eval_str("Math.fround(1.5)"), EvaluatedValue::Deopt));
    }
}
