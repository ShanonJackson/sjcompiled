# Phase 8b — Compose agent write-up

> Lifecycle-correct composition of the 12 plugins reachable from
> `packages/css/src/transform.ts:32-100` lives in
> `crates/css/src/transform.rs`. This document records the public-API
> shape, drift flags raised during the port, and open questions for the
> NAPI agent.

## Public API shape

```rust
pub struct TransformOpts {
    pub optimize_css: Option<bool>,             // serde "optimizeCss"
    pub class_name_compression_map: Option<IndexMap<String, String>>,
    pub increase_specificity: Option<bool>,     // serde "increaseSpecificity"
    pub sort_at_rules: Option<bool>,            // serde "sortAtRules"
    pub sort_shorthand: Option<bool>,           // serde "sortShorthand"
    pub class_hash_prefix: Option<String>,      // serde "classHashPrefix"
}

pub struct TransformResult {
    pub sheets: Vec<String>,
    pub class_names: Vec<String>,               // serde "classNames"
}

pub fn transform_css(css: &str, opts: &TransformOpts)
    -> Result<TransformResult, String>;
```

The `Result<_, String>` shape mirrors `crates/css/src/sort.rs`'s pattern:
parse errors and plugin errors propagate as the inner `Err(String)`. The
NAPI agent should wrap these via `napi::Error::from_reason(...)`,
matching the JS `transformCss` try/catch in `transform.ts:84-99` (or
re-emit the exact `createError('css', 'Unhandled exception')(...)`
message format if any consumer parses error strings — see open
question #2 below).

## Composition recipe (1:1 with `PHASE_8B_LIFECYCLE_AUDIT.md`)

The body of `transform_css` follows the round-by-round recipe verbatim:

### Round 1 — Once round (plugin-array order)

1. `discardDuplicates.Once` → `compiled_css::plugins::discard_duplicates`
2. `parentOrphanedPseudos.Once` → `compiled_css::plugins::parent_orphaned_pseudos`
3. `sortAtomicStyleSheet.Once` → `compiled_css::plugins::sort_atomic_style_sheet`
   — load-bearing position: array index 9 but Once-round.

### Round 2 — Walk round

1. `postcss-nested.Rule` (only Rule visitor) → `postcss_nested::postcss_nested`
   with `bubble = ['container', '-moz-document', 'layer', 'else',
   'when', 'starting-style']` and `unwrap = ['color-profile',
   'counter-style', 'font-palette-values', 'page', 'property']`.
   Runs as a tree sweep, matching the audit's "Walk round" Plugin 4
   classification (no overlap with any other walk-round visitor — it's
   the only Rule visitor in the array).
2. **The three Declaration visitors interleave per-node in a single
   hand-rolled DFS** (`interleaved_decl_walk` in `transform.rs`).
   At every Declaration node, the visitors fire in plugin-array order:
   - `discardEmptyRules.Declaration` (#2) — `is_value_empty(decl.value)`
     check ported from `compiled_css::plugins::discard_empty_rules::is_value_empty`
     (newly exposed `pub`); on hit, the decl is removed and subsequent
     visitors at this slot are short-circuited (matching postcss's
     `if (!node.parent) { stack.pop() }` in `lazy-result.js::visitTick`).
     The parent-rule-removal branch (`packages/css/src/plugins/
     discard-empty-rules.ts:17-19`) is handled by the outer container
     recursion: after processing all children, if any decl was removed
     and the parent is now an empty Rule (not AtRule), the parent is
     dropped — same logical position as upstream's `parent.remove()`
     after `node.remove()`.
   - `normalize-current-color.Declaration` (#5o, only when
     `optimize_css = true`) — calls
     `compiled_css::plugins::normalize_current_color::process_declaration`
     (newly exposed `pub`). Mutates `decl.value` in place; never
     adds/removes nodes.
   - `expandShorthands.Declaration` (#6) — calls
     `compiled_css::plugins::expand_shorthands::process_declaration`
     (newly exposed `pub`). Returns `Some(Vec<Node>)` if the prop
     matches a shorthand and the value is safe to expand; the merged
     walker then routes through `replace_with_at(parent, i, new_decls)`,
     advancing the cursor past the new longforms (matching
     `walk_mut`'s `Mutation::ReplaceMany` index advance).

   This matches the audit's "Composition recipe → Walk round → At each
   Declaration node" specification verbatim. The previous Phase 8b draft
   ran these as three sequential sweeps with a documented byte-equivalence
   argument; per CLAUDE.md drift-detection rules, "byte-equivalent in
   theory" is not acceptable, so the walk now fires per-node-merge
   exactly as JS postcss does.

   **Per-node-merge proof of correctness** (see
   `per_node_interleave_*` tests in `transform.rs`):
   - `per_node_interleave_normalize_then_expand_at_shorthand` — input
     `margin: currentcolor` produces 4 longforms with canonical
     `currentColor` casing, proving normalize fired before expand at
     the same decl.
   - `per_node_interleave_discard_short_circuits_expand` — input
     `margin: undefined` (a shorthand prop with empty value) produces
     zero output, proving discard's removal short-circuited expand.
   - `per_node_interleave_drops_parent_rule_when_emptied` — input
     `:hover { margin: undefined; }` produces zero output, proving the
     outer container recursion correctly removes the emptied parent
     rule.
   - `per_node_interleave_commutativity_proof` — documents that the
     three visitors are independent at the per-decl level (different
     predicates) and the only cross-visitor flow is normalize → expand
     (which the array order forces in BOTH interleaved and sequential
     arrangements, so the observed expand input is byte-identical
     either way). The interleave is required by the spec regardless,
     because future plugins added at the same Declaration slot could
     introduce non-commutative dependencies.

### Round 3 — OnceExit round (plugin-array order)

1. **14 cssnano sub-plugins** (filtered to BASE ∪ PROD by
   `optimize_css`) iterated through `cssnano_preset_default::default_preset()`
   in **cssnano-preset-default source order** (Anomaly #7). The filter
   list is re-derived in `transform.rs` as `NORMALIZE_BASE_PLUGINS` /
   `NORMALIZE_PROD_PLUGINS` to match `normalize-css.ts:13-50`.
2. `atomicifyRules.OnceExit` — collects emitted class names into
   `atomicify_opts.class_names: Vec<String>`. **Mirrors the JS callback
   semantics** (push every emitted class).
3. `increaseSpecificity.OnceExit` — gated by `opts.increase_specificity`.
4. `autoprefixer.OnceExit` — gated by
   `std::env::var("AUTOPREFIXER").map(|v| v != "off").unwrap_or(true)`
   (string equality with the literal `"off"`, per audit). Wires
   `autoprefixer::autoprefixer::build_prefixes_default(None)` →
   `Processor::{remove,add}` in that order, matching `autoprefixer.js`
   lines 126-134.
5. `postcss-normalize-whitespace.OnceExit` → `postcss_normalize_whitespace::postcss_normalize_whitespace`.
6. `extractStyleSheets.OnceExit` — collects sheets into
   `extract_opts.sheets: Vec<String>`.

### Final shape

- `class_names = unique(&raw_class_names)` — matches
  `transform.ts:82` (`classNames: unique(classNames)`).
- `sheets = extract_opts.sheets` — NOT deduplicated, matches
  `transform.ts:81`.

## New public surface added in `compiled-css`

To enable the per-node walk-round merge in `transform.rs`, three new
per-Declaration entry points were added to existing plugins. The
existing root-scoped entry points are unchanged so Phase 6 BAND
consumers (parity-runner stage `cssnano-band`) and the existing
`normalize_css` orchestrator remain untouched:

- `compiled_css::plugins::discard_empty_rules::is_value_empty(value: &str) -> bool`
  — promoted from `fn` to `pub fn`. Mirrors upstream's local
  `isValueEmpty` lambda. The merged walker uses it as the predicate
  for visitor #2's empty-decl removal branch. Parent-rule removal
  (#2's second mutation) is handled directly by the merged walker
  in `transform.rs::interleaved_decl_walk`'s outer recursion — same
  logical position as upstream's `parent.remove()` after `node.remove()`.

- `compiled_css::plugins::normalize_current_color::process_declaration(node: &mut Node)`
  — new `pub fn`. Mirrors the body of upstream's
  `Declaration(declaration)` visitor: in-place value rewrite when
  `decl.value.toLowerCase() === 'currentcolor' || 'current-color'`.
  No-op for non-Declaration nodes so callers can pass any node from
  the walker without pre-checking.

- `compiled_css::plugins::expand_shorthands::process_declaration(node: &mut Node) -> Option<Vec<Node>>`
  — new `pub fn`. Mirrors the body of upstream's `Declaration(decl)`
  visitor: returns `Some(new_decls)` when the decl matches a shorthand
  prop and the value is safe to expand (no `var(...)`); `None`
  otherwise. The caller is responsible for calling `decl.replaceWith(...)`
  / `Mutation::ReplaceMany(new_decls)` semantics.

The audit also flagged that `compiled_css::plugins::normalize_css::normalize_css`
packages two distinct lifecycle hooks (Declaration walk +
14 cssnano OnceExits) into one call — and warned the compose agent
must NOT call it as a single function from the outer pipeline.

**We honored that warning** by NOT calling `normalize_css` from
`transform.rs`. Instead:
- The `normalize_current_color` Declaration visitor is invoked per-decl
  via the new `process_declaration` entry point listed above (during
  the merged walk round).
- The 14 cssnano OnceExits are iterated directly via
  `cssnano_preset_default::default_preset()` filtered by name (already
  a public entry).

The filter logic (`NORMALIZE_BASE_PLUGINS` / `NORMALIZE_PROD_PLUGINS`)
is re-derived literally in `transform.rs` rather than re-exported from
`compiled_css::plugins::normalize_css` because the latter declares them
as `const &[&str]` private to that module.

`compiled_css::plugins::normalize_css::normalize_css(...)` remains a
unit for Phase 6 BAND consumers without modification.

## Drift detection — flagged items

### 1. Walk round now matches the audit verbatim

**Previously flagged as a "byte-equivalent departure"; now resolved.**
The original Phase 8b draft ran the three Declaration visitors as
three sequential sweeps in plugin-array order, with an inline
equivalence argument that the visitors are independent at the
per-decl level. Per CLAUDE.md drift-detection rules ("byte-equivalent
in theory" is not acceptable; spec compliance must be verbatim), the
walk has been rewritten to do a SINGLE depth-first walk where, at
each Declaration node, all three matching visitors fire in array
order before moving to the next node — exactly what the audit's
"Composition recipe → Walk round" specifies.

The three new public entry points listed above expose the per-decl
visitor logic; `transform.rs::interleaved_decl_walk` orchestrates
the per-node interleave including the parent-rule-removal branch
of `discardEmptyRules` and `Mutation::ReplaceMany` semantics for
`expandShorthands`. The four `per_node_interleave_*` tests prove the
new behaviour matches the spec.

### 2. AUTOPREFIXER env equality check is exact

`std::env::var("AUTOPREFIXER").map(|v| v != "off").unwrap_or(true)`
mirrors `process.env.AUTOPREFIXER === 'off'` exactly:
- unset → enabled (matches JS where `process.env.AUTOPREFIXER` is
  `undefined` and `undefined !== 'off'` is `true`).
- empty string `""` → enabled (`"" !== 'off'` is `true`).
- exact `"off"` → disabled.
- any other value (`"OFF"`, `"on"`, `"0"`, etc.) → enabled.

### 3. No new drift detected outside compose-agent territory

Re-verified per-plugin public APIs match the audit's spec:
- `discard_duplicates` / `discard_empty_rules` / `parent_orphaned_pseudos`
  / `expand_shorthands` / `normalize_current_color` /
  `increase_specificity` / `extract_stylesheets` — `pub fn x(root)` shape.
- `sort_atomic_style_sheet` — `pub fn(root, opts)` with `Option<bool>`
  fields preserving the JS `?? true` semantics inside the plugin.
- `atomicify_rules` — `pub fn(root, &mut opts)` with `class_names: Vec<String>`
  collected on opts. Matches the audit's "callback emission via mutable
  collection" shape.
- `postcss_nested` — `pub fn(root, opts)` with the v5.0.6
  bubble/unwrap/preserve_empty config knobs. Matches Anomaly #1.
- `postcss_normalize_whitespace` — `pub fn(root)` with internal
  per-call cache.
- `autoprefixer::build_prefixes_default(from)` + `Processor::new(prefixes)`
  + `Processor::{remove,add}(root, warnings)` — matches Phase 7's
  Phase 8a parity-runner integration (autoprefixer NAPI shim already
  uses identical wiring).

## Test coverage

`crates/css/src/transform.rs` ships unit tests covering:

- `empty_input_returns_empty_lists` — `""` → `{ sheets: [], classNames: [] }`.
- `simple_decl_emits_one_atomic_class_and_sheet` — basic atomicify path.
- `multi_decl_emits_one_class_per_decl` — plural emission.
- `duplicate_decl_is_deduplicated_to_one_class_post_unique` —
  discardDuplicates.Once gate + unique() dedup.
- `rule_with_decl_emits_class_keyed_to_selector` — selector context.
- `increase_specificity_off_by_default` — conditional gate off.
- `increase_specificity_on_appends_not_marker` — conditional gate on.
- `autoprefixer_off_env_disables_prefixer` — env "off" path.
- `autoprefixer_on_env_runs_prefixer` — env unset path.
- `optimize_css_false_skips_normalize_current_color` — walk-round
  gating.
- `optimize_css_default_normalizes_current_color` — walk-round
  active.
- `optimize_css_false_skips_cssnano_prod_plugins` — OnceExit-round
  gating (colormin is PROD-only; `#ff0000` survives when off).
- `nested_rule_unwrapped_by_postcss_nested` — walk-round Rule visitor.
- `empty_decl_value_dropped_by_discard_empty_rules` — walk-round
  Declaration visitor.
- `shorthand_expanded_by_expand_shorthands` — `margin: 1px` → 4
  longforms.
- `parent_orphaned_pseudos_prepends_nesting_selector` — Once-round
  selector rewrite.
- `callback_dedup_preserves_order` — unique() ordering.
- `class_hash_prefix_applied` — opts threading.
- `class_hash_prefix_invalid_returns_error` — atomicify validation.
- `parse_error_propagates` — error path soft-asserted.

**Per-node walk-round interleave tests** (added when the walk round
was rewritten to match the audit's per-node-merge spec):

- `per_node_interleave_normalize_then_expand_at_shorthand` — input
  `margin: currentcolor` produces 4 longforms with canonical
  `currentColor` casing, proving normalize-current-color (#5o) fires
  before expandShorthands (#6) at the same decl.
- `per_node_interleave_discard_short_circuits_expand` — input
  `margin: undefined` (a shorthand prop with empty value) produces
  zero output, proving discardEmptyRules (#2) removing the decl
  short-circuits expand at the same slot — matches postcss's
  `if (!node.parent) { stack.pop() }` in `lazy-result.js::visitTick`.
- `per_node_interleave_drops_parent_rule_when_emptied` — input
  `:hover { margin: undefined; }` produces zero output, proving the
  outer container recursion correctly drops the emptied parent rule
  in the same logical position as upstream's `parent.remove()` after
  `node.remove()`.
- `per_node_interleave_commutativity_proof` — empirical evidence that
  the three Declaration visitors are commutative on every realistic
  input we can construct (acts as a regression on the spec: if a
  future plugin is added at the same Declaration slot and breaks
  commutativity, this test framework is the right place to capture
  the interleave-sensitive case).

Run results: `cargo test -p css --no-fail-fast` → **27/27 pass** (the 3
sort tests + 24 transform tests). `cargo test -p compiled-css` → **119/119
pass** (no regression on the existing root-scoped plugin entries; new
per-decl entry points compile alongside).

The AUTOPREFIXER env-var tests serialize on a `Mutex<()>` to avoid
parallel-test interference; this is best-effort and any future test
that reads/writes that env var should also take the lock.

## Open questions for the NAPI agent

### 1. Callback marshalling — JS callbacks vs returned Vecs

JS `transformCss` accepts no callbacks from outside; the two internal
callbacks (`atomicifyRules`'s class-name push and `extractStyleSheets`'s
sheet push) are private to the function and resolved before return.
The Rust port collects both into `Vec<String>`s on the return value
(`TransformResult { sheets, class_names }`). **No NAPI marshalling of
JS callbacks is needed** — the NAPI shim should just construct the
JS-shaped object and return it synchronously, matching the
`{ sheets: string[]; classNames: string[] }` return shape.

### 2. Error wrapping format

`transform.ts:84-99` wraps any thrown error in:

```text
An unhandled exception was raised when parsing your CSS, this is probably a bug!
  Raise an issue here: https://github.com/atlassian-labs/compiled/issues/new?...

  Input CSS: {
    ${css}
  }

  Exception: ${message}
```

The Rust `transform_css` returns the inner `Err(String)` raw (e.g.
`"parse error: ..."` or `"atomicify-rules: ..."`). **Should the NAPI
agent reconstruct the `createError('css', 'Unhandled exception')`
wrapper byte-for-byte?**

If any AFM consumer parses the error message text (low likelihood per
the audit), yes. Otherwise the NAPI shim can simply
`napi::Error::from_reason(rust_err_string)` and skip the wrapper.
Recommendation: have the NAPI shim reproduce the wrapper exactly to
match upstream behavior under all conditions, since the cost is
trivial.

### 3. `classNameCompressionMap` shape on the JS side

`packages/css/src/transform.ts:19` types it as
`Record<string, string>`. Rust models it as
`Option<IndexMap<String, String>>` to preserve insertion order (matches
JS Object insertion order semantics for string keys per V8's spec —
same reasoning as `discardDuplicates`'s IndexMap, see audit Plugin 1
DRIFT RISK note).

The NAPI agent will likely receive this as a `napi::Object` from JS;
the marshalling needs to preserve insertion order via
`object.get_property_names()` and iterate in that order. `napi-rs`
returns names in the JS-spec ordering, so this should be safe — but
verify with a test that inserts multiple keys and confirms the order
flows through to the atomicify lookup.

### 4. `BROWSERSLIST` env pin

The audit's hazard #10 calls out that autoprefixer + 5 cssnano
sub-plugins resolve browserslist at runtime. The Phase 6 BAND parity
runner pins `BROWSERSLIST=chrome 100` for both engines. The NAPI agent
should ensure the parity gate for the full transform pipeline pins the
same env var (or whatever AFM uses in production) so both engines see
the same browser list on every call. This is parity-test infrastructure,
not Rust-side code.

### 5. `result.opts.from` propagation for autoprefixer

`autoprefixer.js:122` reads `result.opts.from` to seed
browserslist's path resolution. In the Rust port, `transform_css`
currently passes `None` to `build_prefixes_default(None)`, which
defaults to `current_dir()` for browserslist resolution.

If AFM passes a source `.css` path to `transformCss` via some side
channel (it doesn't today — `transform.ts:74` hardcodes
`from: undefined`), the NAPI agent will need to thread it through.
For now `from = None` matches `transform.ts:74` exactly — `from:
undefined` in JS yields `result.opts.from = undefined` which makes
autoprefixer fall back to `process.cwd()`. The Rust equivalent is
`current_dir()`. **Byte-equivalent under matching cwd.** Pin the cwd
during parity tests.

### 6. AUTOPREFIXER env-var test serialisation

The compose layer's tests serialise on a `Mutex<()>` because Rust tests
run in parallel and `std::env::set_var` is process-global. The NAPI
shim doesn't need this — it just reads the env var on each call. But
**parity-runner end-to-end tests that toggle AUTOPREFIXER must still
serialise**, because the env-var lookup happens at the Rust call site
mid-test. Document this in the parity-runner's stage handler if not
already.
