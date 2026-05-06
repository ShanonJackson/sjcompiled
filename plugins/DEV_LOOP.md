# `plugins/DEV_LOOP.md` — fixture divergence triage runbook

> **Purpose:** the practical Find → Fix → Verify → Loop playbook for closing
> `babel-plugin` port divergences against the `/fixtures` corpus. Read
> `CLAUDE.md` first; this file is the **how**, that file is the **why**.
> Read `plugins/FIXTURES_STATUS.md` for the **current state** of the corpus.
>
> **Audience:** the next agent (and the agent after that). This loop is the
> primary work surface for the rest of the babel-plugin port. Every minute
> you save here compounds.

---

## The four-step loop

```
┌─────────┐     ┌────────────────┐     ┌──────────┐     ┌──────────┐
│  FIND   │ ──> │  FIX           │ ──> │  VERIFY  │ ──> │  LOOP    │
│ a small │     │ matching       │     │ no       │     │ next     │
│ diverge │     │ upstream 1:1   │     │ regress  │     │ smallest │
└─────────┘     └────────────────┘     └──────────┘     └──────────┘
```

The whole loop should be **< 30 minutes per fixture** once you're in flow.
If you're spending more, you're missing one of the techniques below.

---

## STEP 1 — FIND: pick the smallest open divergence

```bash
# Refresh the corpus snapshot.
bun parity-harness/fixtures-triage.mjs
```

Read `plugins/FIXTURES_STATUS.md` "Open divergences" section. Rules:

1. **Smallest input first.** Sort `ct-*` divergences by `wc -l fixtures/*/input.*`. Three lines of TSX is two orders of magnitude faster to triage than a hundred.
2. **Skip the deferred clusters.** Sheet-ordering (5 fixtures) is blocked on `transform_cache` port. `ct-hover-display` is blocked on a scope-index architectural decision. Those are gated; don't burn time on them.
3. **Avoid `ct-*-massive` / `ct-editor-big` until last.** They run, but the diff output truncates and bisecting takes longer.

Get the byte-level diff for one fixture:

```bash
bun parity-harness/fixtures-triage.mjs --only <name> --print-diffs 2>&1
```

The diff prints `divergence at byte N (a.length=X, b.length=Y)` followed by a 120-byte slice from each side around the offset. **That's your first signal.**

---

## STEP 2 — FIX: 1:1 with upstream, never around it

This is the step where drift gets introduced. Follow the sequence.

### 2.1 — Dump both outputs FULLY

The `--print-diffs` slice is 120 bytes. You usually need the whole file. Write a one-shot dump script (don't commit it):

```javascript
// parity-harness/_dump.mjs (delete after use)
import { babelEngine, swcEngine } from './babel-plugin/engines.ts';
import { readFileSync } from 'node:fs';
const file = process.argv[2];
const src = readFileSync(file, 'utf8');
const opts = { filename: file };
console.log('=== BABEL ===');
console.log(babelEngine(src, opts));
console.log('=== SWC ===');
console.log(swcEngine(src, opts));
```

```bash
bun parity-harness/_dump.mjs fixtures/<name>/input.tsx
```

Read both outputs side by side. The minimal question: **what does Babel emit that we don't, structurally?** Catch-all CSS-variable (`var(--_xyz)`) on our side vs atomic class hashing on Babel's is the signature of a missed optimization gate. Class name suffix differences with same atom shape is the signature of a hash-input drift. Different declaration ORDER with same atoms is the deferred sheet-ordering cluster.

### 2.2 — Find the upstream code path FIRST, port code SECOND

**Order of operations is non-negotiable:**

1. Read upstream first (`packages/babel-plugin/src/...`). Trace from `babel-plugin.ts` (the visitor entry) down to whichever helper produces the divergent output. Use `Grep` for the function names you find along the way.
2. Read the matching Rust port (`crates/babel-plugin/src/...`). The layout is 1:1 — `packages/babel-plugin/src/utils/css-builders.ts` ↔ `crates/babel-plugin/src/utils/css_builders.rs`, etc.
3. Diff the two by **semantics**, not line numbers. Babel uses `t.isX(node)` checks; SWC uses `Expr::X(...)` matches. Babel uses `path.node[c]`; the Rust port unwraps from a `&Expr` directly.
4. Identify the delta. Common shapes:
   - **Wrong constants** (e.g. `CONDITIONAL_PATHS` was widened from `['consequent', 'alternate']` to include `'test'` — see `manipulate_template_literal.rs` history).
   - **Auto-positive recursion** where upstream matches direct children only (e.g. `LogicalExpression` flagging anywhere in subtree vs only at `Cond.cons` / `Cond.alt`).
   - **Missing `Paren` unwrap** (SWC keeps `Expr::Paren`; Babel's parser strips it).
   - **Missing `TsAs` / `TsConstAssertion` unwrap** (Babel's `t.isTSAsExpression` covers both shapes; SWC splits them).
   - **Stale snapshot** in scope_index after an in-place rewrite (e.g. `ct-hover-display`).

### 2.3 — When upstream and Rust look like they should agree but don't, instrument upstream

The breakthrough trick. Babel's source IS the oracle — and you can write to it temporarily.

**Allowed exception to `packages/*` immutability per `CLAUDE.md`:** add `console.error('UPSTREAM-DBG …')` lines to a helper, scoped to the specific input (e.g. `node.quasis.some(q => q.value.raw.includes('min-height'))` to fire only on the fixture you're triaging). Run the dump. **Revert before committing.**

```typescript
// e.g. in packages/babel-plugin/src/utils/manipulate-template-literal.ts
const _dbg = node.quasis.some((q) => q.value.raw.includes('min-height'));
if (_dbg) console.error('UPSTREAM-DBG parent.type:', parent?.type);
```

This single technique resolved the `ct-minheight-calc-fg-stack` divergence in this session by revealing that upstream's `parent` was a `CallExpression`, not a synthetic ExpressionStatement, and that `CONDITIONAL_PATHS.map` only visited `consequent`/`alternate` — never `test`. Without the printf, that fact takes hours to deduce from reading code.

**Always revert** the upstream debug prints before running the verification step. Diff `git status packages/babel-plugin/src/` to confirm.

### 2.4 — When the WASM plugin's behavior is opaque, write a cargo integration test

`eprintln!()` from inside the WASM plugin **does** reach the harness stderr — but printing on every transform across 336 fixtures DDOSes the loop, and printing inside a transformer that recurses heavily can cause output buffering hangs.

**Better:** write a 30-line cargo integration test. They run native, `eprintln!` works perfectly, and you get full structured `{:#?}` AST dumps without WASM in the loop.

Pattern:

```rust
// crates/babel-plugin/tests/_debug_<bug>.rs (delete after use)
use babel_plugin::run_dispatcher;
use babel_plugin::types::PluginOptions;
use std::sync::Arc;
use swc_core::common::comments::SingleThreadedComments;
use swc_core::common::sync::Lrc;
use swc_core::common::{FileName, SourceMap};
use swc_core::ecma::ast::EsVersion;
use swc_core::ecma::parser::{parse_file_as_program, Syntax, TsSyntax};

#[test]
fn debug_my_bug() {
    let src = r#"<paste fixture/input.tsx contents>"#;
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Arc::new(FileName::Real("input.tsx".into())),
        src.to_string(),
    );
    let comments = SingleThreadedComments::default();
    let mut errs = vec![];
    let mut program = parse_file_as_program(
        &fm,
        Syntax::Typescript(TsSyntax { tsx: true, ..Default::default() }),
        EsVersion::Es2022,
        Some(&comments),
        &mut errs,
    ).expect("parse ok");
    let _v = run_dispatcher(&mut program, PluginOptions::default(), comments);
    eprintln!("=== AFTER ===\n{:#?}", program);
}
```

Run with: `cargo test -p babel-plugin --test _debug_my_bug -- --nocapture | grep DBG`.

Add targeted `eprintln!("DBG …")` lines inside the Rust port itself, gated on the input shape (e.g. `if key == "minHeight"`). Iteration time is `cargo test` ~5s vs full `cargo build --target wasm32-wasip1 --release` + `bun parity-harness/...` ~30s.

**Delete the test file before committing.** It's noise in the parity-gate suite.

### 2.5 — Apply the fix in the SAME folder/file as upstream

Per `CLAUDE.md`: "the ONLY way to do this correctly is to migrate everything 1:1 same folder/file system as ORIGINAL." That means:

| Upstream                                              | Rust port                                                |
|-------------------------------------------------------|----------------------------------------------------------|
| `packages/babel-plugin/src/utils/css-builders.ts`     | `crates/babel-plugin/src/utils/css_builders.rs`          |
| `packages/babel-plugin/src/utils/manipulate-template-literal.ts` | `crates/babel-plugin/src/utils/manipulate_template_literal.rs` |
| `packages/babel-plugin/src/utils/constants.ts`        | (inlined as `const` next to the consumer in Rust)        |

If you find yourself wanting to add the fix in a *different* file (e.g. a `compat/` shim or a new "drift adapter"), **stop**. That's drift you're hiding from a future agent. Find the upstream call site and port THAT.

The only legitimate exceptions are documented in `crates/babel-plugin/src/compat/`:
- `compat/paren.rs` — SWC keeps `Expr::Paren` that Babel's parser strips.
- `compat/template_literal_raw.rs` — SWC parser preserves CRLF in `TplElement.raw`; Babel normalises CR/CRLF → LF per ES §12.8.6.
- `compat/scope.rs` / `compat/path.rs` — `@babel/traverse`'s `path.scope.getBinding` etc. don't exist in SWC's plugin runtime.
- `compat/evaluation.rs` — port of `@babel/traverse/lib/path/evaluation.js`.
- `compat/generator/` — port of `@babel/generator`.

**Adding a new `compat/*.rs` file requires the same justification:** a SWC↔Babel API delta that can't be papered over in the call site. Not a code-smell escape hatch.

### 2.6 — Drift-check your own fix

This is where I (the previous agent) got caught in this session. You will too. Before declaring done, re-read your diff and ask:

- **Did you weaken any check upstream had?** E.g. matching `Expr::TaggedTpl(_)` (any) instead of `tagged.tpl == node` (identity) is drift, even if it passes the corpus today. The `CLAUDE.md` rule "DON'T try to WORK AROUND drift" applies to your own port code, not just to upstream behaviour.
- **Did you add coverage upstream doesn't have?** E.g. a `_ => match more cases` arm that Babel never reaches. Conservative-extra is still drift.
- **Did you copy a comment from the OLD Rust code that asserted upstream did X, when upstream actually does Y?** Comments lie when the code below them changes.

If you catch drift in your own fix, say so explicitly in the PR description ("Drift in my own fix detected: <description>; corrected by <change>"). Future agents auditing the file history will thank you.

---

## STEP 3 — VERIFY: three gates, in order

### 3.1 — Single-fixture verification (~5s)

```bash
cargo build -p babel-plugin --target wasm32-wasip1 --release
bun parity-harness/fixtures-triage.mjs --only <name> --print-diffs 2>&1 | tail -20
```

If `divergence=0 parity=1` → primary fix works. If still `divergence=1` → don't move on; iterate.

### 3.2 — Full `/fixtures` corpus regression (~30s)

```bash
bun parity-harness/fixtures-triage.mjs
```

Compare the `parity` count against the count you started from. **If it decreased, you broke something.** The harness names the regressing fixture; loop back to STEP 2 with that as the new triage target.

### 3.3 — JS-extracted unit-test corpus regression (~30s)

This is the `Phase 6 §6.5` lock at **476/477 parity**. Independent oracle from `/fixtures` — uses the unit-test source from `packages/babel-plugin/src/**/*.test.ts` directly.

```bash
# First time per branch (extracted fixtures are gitignored):
bun parity-harness/babel-plugin/extract-fixtures.mjs

# Each subsequent run:
bun parity-harness/babel-plugin/triage.mjs
```

Expected:

```
total                477
parity               476
divergence           0
swc-throws           0
babel-throws         0
both-throw           1     ← negative-test fixture
```

If extraction prints `Imported 1 files (26 failed). Wrote 0 fixtures` with `Expected "from" but found "{"` errors, the Bun version's plugin-loader path is rejecting TS syntax. The extractor (as of 2026-05-07) runs the rewritten `test-utils.ts` through `Bun.Transpiler` and returns `loader: 'js'` to dodge this. If that fix regresses, it's in `parity-harness/babel-plugin/extract-fixtures.mjs` ~line 175.

**Both `/fixtures` and unit-test corpora must hold.** They are independent oracles — a regression in one but not the other is a real signal of a partial fix.

### 3.4 — Cargo lib + integration tests (~3min)

```bash
cargo test -p babel-plugin --release
```

Expected: 484+ pass. The single known failure as of 2026-05-07 is `resolver::engine::tests::build_from_config_with_transforms_doesnt_break_default_resolution` (`NotFound("parity-pkg-main-only")`) — pre-existing, unrelated to the plugin port. If you see new failures, they're yours.

---

## STEP 4 — LOOP: keep momentum

### 4.1 — Update `plugins/FIXTURES_STATUS.md`

Move the entry from "Open divergences" to "Closed". Use this template:

```markdown
- **[FIXED YYYY-MM-DD] <fixture-name>(s)** — <one-line root cause>.
  <2-4 sentences explaining the upstream semantics, what the Rust port
  was doing wrong, and the specific file/function that changed>.
  +N parity.
```

Bump the `parity` and `divergence` counts in the snapshot block. Don't leave stale numbers.

### 4.2 — If your fix closed multiple fixtures, name them all

The `manipulate_template_literal.rs` fix in this session closed THREE fixtures (`ct-minheight-calc-fg-stack`, `ct-columns-container-minheight-stack`, `ct-styled-nth-of-type-container`) because they shared a root cause. Same-cause fixtures clustered into one entry are easier to audit later than three separate entries.

### 4.3 — If you find drift OUTSIDE your task, raise it

Per `CLAUDE.md`: **"If you think someone hasn't ported something OUTSIDE your work CORRECTLY; Immediately I.E 'Drift detected in X — <Explanation>'."**

Don't fix it silently. Don't even fix it loudly without permission. Just flag it. Drift fixes that aren't part of an explicit task accumulate uncontrolled.

### 4.4 — Clean up your scratch files

Before the next loop:

```bash
# Delete temp dumpers / debug tests
rm -f parity-harness/_dump*.mjs
rm -f crates/babel-plugin/tests/_debug_*.rs

# Confirm packages/babel-plugin/src/ is clean (no leftover console.error)
git diff packages/babel-plugin/src/

# Confirm crates/babel-plugin/src/ has no leftover eprintln!("DBG ...")
rg 'eprintln!\("DBG' crates/babel-plugin/src/
```

The corpus is going to run hundreds of times after you ship. Stray `eprintln!` in a hot path (`extract_template_literal` runs per template-literal interpolation per fixture) blows up triage runtime and pollutes stderr.

---

## Anti-patterns (don't)

### "I'll add a reconciler in `engines.ts` to massage the output"

Reconcilers are for **host-environment** deltas only — SWC's pipeline mutating the AST after our plugin exits (e.g. JSX-runtime ordering, hygiene renames, react-classic spread collapse). They are NEVER for plugin-output divergences.

If you find yourself wanting to write a reconciler that strips `var(--_xyz)` from the SWC side and replaces it with the atomic class, **stop**. That's drift you're hiding. Find the missing optimization gate in the plugin and port it.

### "The babel comment says X but the code does Y; I'll trust the comment"

Trust the code, always. Babel's comments are informational and occasionally stale. The output bytes are the contract.

### "I'll add a `_ => fallback` arm to be safe"

Conservative is drift. If upstream returns `false` / `None` / no-op for an unhandled shape, the Rust port should too. Adding a `_ => /* defensive thing */` arm hides bugs that would otherwise show up in the corpus.

### "The fix is in `compat/` because X is hard to do in the visitor"

`compat/` is for irreconcilable SWC↔Babel runtime API gaps (parens, template raws, scope/path API, evaluation, generator). It is NOT for "I couldn't figure out where to put this." Find the upstream call site and port it there.

### "I'll silence the divergence by editing the fixture"

`/fixtures/*` is **frozen**. Adding new fixtures is fine; editing existing ones to make them pass is forbidden. The corpus is the oracle; mutating the oracle invalidates every prior parity claim.

### "I'll commit the debug `eprintln!` because it's gated on a key name"

The corpus runs in CI. Stray `eprintln!` in a hot path is a perf regression even when gated, and stderr noise drowns out real failures. Strip every `DBG` print before committing.

---

## Tooling cheat-sheet

```bash
# Full triage: /fixtures corpus (293 single-file + skips multi-file).
bun parity-harness/fixtures-triage.mjs

# Triage with --include-multi (43 ct-* multi-file; gated on §5.4–§5.6).
bun parity-harness/fixtures-triage.mjs --include-multi

# Single fixture, with byte diff printed inline.
bun parity-harness/fixtures-triage.mjs --only <name> --print-diffs

# Bail on first divergence (handy when iterating on a fix that may regress).
bun parity-harness/fixtures-triage.mjs --bail

# Phase 6 §6.5 unit-test corpus extract+run.
bun parity-harness/babel-plugin/extract-fixtures.mjs
bun parity-harness/babel-plugin/triage.mjs

# Rebuild the WASM plugin (required after any crates/babel-plugin/ change).
( cd crates && cargo build -p babel-plugin --target wasm32-wasip1 --release )

# Cargo lib+integration tests for the plugin crate.
cargo test -p babel-plugin --release

# Find every `eprintln!("DBG"` left in the source tree.
rg 'eprintln!\("DBG' crates/

# Find every `console.error('UPSTREAM-DBG'` left in upstream.
rg "console\.error\('UPSTREAM-DBG" packages/
```

---

## Time budget per loop

| Step | Target | If you're over, do this |
|------|--------|-------------------------|
| Find smallest divergence | 1 min | You're rereading FIXTURES_STATUS.md too often. Open it once, commit a triage order. |
| Dump both outputs | 1 min | `_dump.mjs` should be a 10-line script you keep in your shell history. |
| Read upstream + Rust side-by-side | 5–10 min | If > 15min, the surface is too wide — bisect the input (delete half the fixture body, re-run, narrow). |
| Write debug test | 5 min | If > 10min, you're rebuilding the parse boilerplate every time. Keep a template. |
| Apply fix | 5–15 min | If > 30min, the upstream code path you're matching is wrong — re-read upstream from a different entry point. |
| Verify (3 gates) | 2 min | Each gate is < 30s. If gates fail, that's a STEP 2 problem, not a STEP 3 problem. |
| Update FIXTURES_STATUS.md | 2 min | Use the template above; copy/paste from a previous closed entry. |
| **Total per loop** | **20–30 min** | If consistently > 1h, escalate to the user with a `Drift detected in X` note. |

---

## When to STOP and ask

- The divergence requires architectural changes (e.g. `transform_cache` port, scope-index live snapshots). Don't start that without a green light.
- The divergence reveals upstream Babel has a real bug. **Don't fix the bug.** Reproduce upstream's behaviour faithfully (BUGS in OLD = BUGS in NEW), file the bug separately for upstream, and document the reproduction in `FIXTURES_STATUS.md`.
- You've spent 1h+ and don't have a fix path. The user would rather know than have you keep grinding.
- You found drift OUTSIDE your assigned divergence. Flag it via "Drift detected in X — <explanation>" and let the user route it.

---

## Final note on quality

Per `CLAUDE.md`: **"Quality is more important than speed."** This loop is fast IF you do it right. Cutting corners (skipping the upstream-instrumentation step, ignoring the `Phase 6 §6.5` gate, "conservative" matchers in your fix) costs more time downstream than it saves now, because every drift compounds when integrated against AFM's 60-90GB monorepo.

The 20–30 min budget assumes you do every step. Skipping steps is how 30 min becomes 3 hours of bug-chasing two weeks later.
