# HANG_BUG_REPORT — `traverse_call_expression` infinite loop on unreadable cross-file imports

You are picking this up fresh. A previous agent confirmed a real, locally
reproducible hang in the SWC `babel_plugin.wasm` plugin. The repro artefacts
are still on disk at `/tmp/_rovodev_symlink_repro/`. Your job is to **find
the actual loop site, fix it faithfully (no drift from upstream JS), and
ship a regression test**.

> **Read `CLAUDE.md` at the repo root before doing anything else.** The
> drift-detection rules are non-negotiable. In particular: do NOT touch
> `packages/babel-plugin`, `packages/babel-plugin-strip-runtime`,
> `packages/css`, or `packages/utils` — those are immutable. The fix
> goes in `crates/babel-plugin/`.

---

## TL;DR

When the SWC plugin's cross-file resolver "successfully resolves" an import
to a path that **the WASI sandbox then fails to read** (because the path
escapes the cwd preopen, e.g. via a symlink whose target sits outside cwd),
the plugin enters an infinite loop instead of deopting cleanly to the
runtime fallback (`var(--…)` + `ix(...)` at the call site).

The loop fires for any expression of the form `<imported>.<prop>(...)`
where `<imported>` resolves through such an unreadable path. In production
(AFM monorepo) this hits **78+ files** that import `layers` from
`@atlaskit/theme/constants`, because the AFM `node_modules/@atlaskit/theme`
is a symlink that escapes the `jira/` cwd preopen.

## Reproducing the hang locally

The previous agent left the full repro layout staged at
`/tmp/_rovodev_symlink_repro/`. If it's been wiped, recreate it with the
recipe in the **"Re-staging the repro"** section below.

```bash
# Hangs forever; perl alarm kills it after 15s.
cd /tmp/_rovodev_symlink_repro/jira
perl -e 'alarm 15; exec @ARGV' bun ../probe.mjs input4.tsx
```

Expected output (currently):

```
cwd = /private/tmp/_rovodev_symlink_repro/jira
fixture = /private/tmp/_rovodev_symlink_repro/jira/input4.tsx
                                      <hangs forever, killed at 15s>
```

For comparison, these adjacent fixtures **do NOT hang** (regression
guards — your fix MUST keep them passing):

| Fixture | Import path | Symlink? | Behaviour |
|---|---|---|---|
| `input.tsx`  | `@atlaskit/theme/constants` (real npm pkg)   | n/a (resolver returns NotFound) | **22ms clean deopt** — `var(--…)` runtime fallback |
| `input3.tsx` | `./local-constants` (sibling, in cwd)        | no                              | **20ms fold** — inlines `z-index:9999` ✓ |
| `input4.tsx` | `./escaping-link` → `../escaped-constants`   | symlink target outside cwd     | **HANG** ← the bug |
| `input5.tsx` | `./escaping-arrow` → `../escaped-…-arrow`    | symlink target outside cwd, arrow shape | **HANG** |
| `input6.tsx` | `./dangling` → `../does-not-exist`           | dangling symlink                | **HANG** |
| `input7.tsx` | `../escaped-constants` (explicit `..` path)  | no symlink                      | **clean deopt** |

The trigger is specifically: **the resolver returns a path it considers
valid, but `std::fs::read_to_string` on that path fails inside the WASI
sandbox**. Both symlink-escapes-cwd AND dangling-target satisfy this. An
explicit `../foo` path the resolver itself rejects up-front does NOT.

### Re-staging the repro (if `/tmp/_rovodev_symlink_repro/` is gone)

```bash
# Clean slate
rm -rf /tmp/_rovodev_symlink_repro
mkdir -p /tmp/_rovodev_symlink_repro/jira/.parity-harness-cache
cp /Users/sjackson3/Documents/sjcompiled/.parity-harness-cache/*.bin \
   /tmp/_rovodev_symlink_repro/jira/.parity-harness-cache/

# Stage @compiled/react (so the plugin recognises styled.div)
mkdir -p /tmp/_rovodev_symlink_repro/jira/node_modules/@compiled/react
cp -r /Users/sjackson3/Documents/sjcompiled/packages/react/* \
      /tmp/_rovodev_symlink_repro/jira/node_modules/@compiled/react/

# The "constants" file lives OUTSIDE the cwd (jira/)
cat > /tmp/_rovodev_symlink_repro/escaped-constants.tsx << 'EOF'
export const layers = {
  card: function card() { return 100; },
  tooltip: function tooltip() { return 9999; },
};
EOF

# Symlink jira/escaping-link.tsx -> ../escaped-constants.tsx
ln -sf ../escaped-constants.tsx \
       /tmp/_rovodev_symlink_repro/jira/escaping-link.tsx

# Hanging fixture
cat > /tmp/_rovodev_symlink_repro/jira/input4.tsx << 'EOF'
import { styled } from '@compiled/react';
import { layers } from './escaping-link';
export const X = styled.div({ zIndex: layers.tooltip() });
EOF

# Probe driver
cat > /tmp/_rovodev_symlink_repro/probe.mjs << 'EOF'
#!/usr/bin/env bun
import { transformSync as swcTransformSync } from '@swc/core';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const REPO_ROOT = '/Users/sjackson3/Documents/sjcompiled';
const BABEL_PLUGIN_WASM = resolve(
  REPO_ROOT,
  'crates/target/wasm32-wasip1/release/babel_plugin.wasm',
);

const fixturePath = resolve(process.cwd(), process.argv[2] || 'input.tsx');
const source = readFileSync(fixturePath, 'utf8');
const cwd = process.cwd().replace(/\\/g, '/');
console.log('cwd =', cwd);
console.log('fixture =', fixturePath);

const t0 = Date.now();
let done = false;
const alarmTimer = setTimeout(() => {
  if (!done) {
    console.log(`\n*** HANG CONFIRMED — still running after ${Date.now() - t0}ms ***`);
    process.exit(2);
  }
}, 25000);

try {
  const result = swcTransformSync(source, {
    filename: fixturePath,
    jsc: {
      target: 'es2022',
      parser: { syntax: 'typescript', tsx: true },
      transform: {
        verbatimModuleSyntax: true,
        react: { runtime: 'classic' },
      },
      preserveAllComments: false,
      experimental: {
        runPluginFirst: true,
        plugins: [[
          BABEL_PLUGIN_WASM,
          {
            root: cwd,
            optimizeCss: false,
            precomputedBrowserslistPath: resolve(cwd, '.parity-harness-cache/browserslist-snapshot.bin'),
            precomputedPrefixesPath: resolve(cwd, '.parity-harness-cache/prefixes-snapshot.bin'),
          },
        ]],
      },
    },
  });
  done = true;
  clearTimeout(alarmTimer);
  console.log(`\nCompleted in ${Date.now() - t0}ms.`);
  console.log('=== OUTPUT ===');
  console.log(result.code);
} catch (e) {
  done = true;
  clearTimeout(alarmTimer);
  console.log(`\nThrew after ${Date.now() - t0}ms:`);
  console.log(String(e).split('\n').slice(0, 10).join('\n'));
}
EOF
```

Note: bun's JS event loop cannot run while the WASM plugin is busy in a
tight loop, so the probe's internal 25s `setTimeout` alarm cannot fire.
Use `perl -e 'alarm N; exec @ARGV' bun …` to externally kill the process
— that's how you confirm a hang vs a slow-but-completing run.

## Background — what the plugin is doing for `<imported>.<prop>(...)`

This is a 1:1 port of `packages/babel-plugin`. The relevant call-site
chain (when called on `layers.tooltip()` as a CSS value):

1. `evaluate_expression(Call(callee=Member(layers, tooltip), args=[]))`
   — entry at `crates/babel-plugin/src/utils/evaluate_expression.rs:328`
   (the `Expr::Call` arm), dispatches to `traverse_call_expression`.

2. `traverse_call_expression`
   (`crates/babel-plugin/src/utils/traverse_expression/traverse_call_expression.rs`):
   - The callee is a `Member`, so it routes to `member_expression_branch`
     at line 190.
   - `member_expression_branch` (lines 371–439) mutates the callee in-place:
     `member.prop = Computed(Call(prop_ident, args))`. This is upstream's
     "tag the property as a CallExpression so the access-path collector
     knows it's a trailing call" trick (mirrors
     `packages/babel-plugin/src/utils/traverse-expression/traverse-call-expression.ts:23-32`).
   - Then calls `evaluate_expression(Member(...))` on the mutated callee
     (line 420).
   - Then there's a "did the evaluator make progress?" structural check
     at lines 425–433 that decides whether to restore `member.prop` to
     its pre-mutation form.

3. `evaluate_expression(Member(...))` re-enters
   (`evaluate_expression.rs:276-321`):
   - First tries `try_namespace_import_dispatch` (for `import * as ns from
     ...` — does NOT apply here).
   - Then tries `try_cross_file_member_dispatch` (line 290, defined at
     line 792). **This is the path that fires for our case** because
     `layers` is a named import.
   - If neither dispatch fires, falls through to
     `traverse_member_expression`.

4. `try_cross_file_member_dispatch` (line 792):
   - Resolves the binding (`layers` — an import).
   - Builds a fresh `ScopeIndex` over the imported module's parsed AST
     (line 828).
   - Re-walks the member chain against the imported file's scope.

5. The imported module's AST comes from `resolve_binding` at line 663:
   ```rust
   || fs::read_to_string(&module_path_for_read).unwrap_or_default(),
   ```
   **When the WASI read fails (ENOTCAPABLE on a path outside the cwd
   preopen), this swallows the error and returns `String::new()`. The
   empty string parses to an empty `Module` (line ~699). The empty module
   has no `layers` export, so the binding lookup returns "unresolved" —
   which surfaces upstream as the input shape unchanged.**

The same `unwrap_or_default()` pattern exists at `evaluate_expression.rs:669`
inside `try_namespace_import_dispatch` (with a separate, more-defensive
`if source.is_empty() && !resolved_path.exists() { return None; }` guard).
The version at `resolve_binding.rs:665` has the equivalent
`exists()`-and-empty guard, but in the WASI sandbox `Path::exists()`
behaviour on an unreadable path is the **load-bearing question** — see
"Working hypotheses" below.

## Where the loop almost certainly is — but verify, don't assume

The previous agent and the bug reporter both pointed at
`traverse_call_expression.rs:419-435` (the structural "progressed" check
that replaces JS's reference-identity guard `if (evaluated.value === callee)`).
**Treat this as a starting point, not a conclusion.** The structural check
itself does not contain a `loop` — it's straight-line code. For the hang
to be unbounded, the loop must be elsewhere AND the broken progressed
guard must be the thing that fails to terminate it.

Suspect call-graph cycles to investigate (pick whichever you can confirm
with a stack trace first; do NOT guess):

- The recursion in `resolve_expression_in_member`
  (`traverse_member_expression/traverse_access_path/resolve_expression/mod.rs:144-158`)
  uses its own `progressed` heuristic
  (`exprs_match_by_kind_and_shape`, lines 177–192) which returns `true`
  for any two `Expr::Call(_)` or `Expr::Member(_)` of the same
  discriminant — meaning `progressed = false` and no recursion. That
  arm looks safe, but worth eyeballing.
- The `try_cross_file_member_dispatch` rebuild + re-evaluate at
  `evaluate_expression.rs:838-859`.
- The `&dispatched` re-entry from `evaluate_expression.rs:217-242`
  ("Phase §5.4e drift-fix consumer contract" — recurses into the
  resolved binding's `node` with a fresh `imported_idx`).
- Any cache interaction in `state.cache.read_file` /
  `state.cache.parse_module` that might hand the same empty module
  back on every iteration.

To find the actual cycle, the fastest approach is to attach a debugger
and grab a backtrace at hang time, or to add temporary `eprintln!`
instrumentation along the suspected paths and rebuild
(`cargo build -p babel-plugin --release --target wasm32-wasip1` —
takes 2-3 minutes per CLAUDE.md).

## Working hypotheses to test

Hypothesis 1 (most plausible):
- WASI's `Path::exists()` returns `true` (or some cached metadata
  state) for the unreadable path, so the
  `if source.is_empty() && !module_path.exists() { return None; }`
  guard at `resolve_binding.rs:665` lets execution flow through into
  parsing the empty string. The empty `Module` is then cached, and
  every subsequent call returns the same empty AST. Some upstream
  caller keeps re-asking for the resolution, never observes a
  change, never terminates.

  **Test**: add a trace inside the WASI cwd-translated path branch
  before line 663 to print `module_path`, `module_path.exists()`, and
  the `read_to_string` `Err` if any. Compare in-cwd vs escaping-symlink
  cases.

Hypothesis 2:
- The cycle is at the `member_expression_branch` callsite inside
  `evaluate_expression.rs:328-345`, where the closure
  `dispatch_evaluate` is recursively invoked. Combined with the
  broken structural progressed guard, the same `Member(Call(...))`
  shape gets re-fed in.

  **Test**: instrument the entry of `member_expression_branch` with a
  per-call counter on the `(member.span.lo, member.span.hi)` pair and
  panic if any pair is seen >100 times.

Hypothesis 3:
- The cache at `meta.state.cache.read_file` returns a stale empty
  string, and the cache at `meta.state.cache.parse_module` returns an
  empty `Arc<Module>`. The empty module's `ScopeIndex` has zero
  bindings, so every binding lookup returns `None`, and some loop in
  the evaluator interprets `None` as "try again with the same input".

  **Test**: dump cache hit counts before/after the hang region.

## What "fixed" looks like

A correct fix MUST satisfy ALL of these:

1. **`input4.tsx` (the hang) completes in well under 1 second.** The
   semantically correct output is the runtime-deopt fallback —
   `_<hash>{z-index:var(--<id>)}` plus `style: { '--<id>': ix(layers.tooltip()) }`
   in the JSX — same shape as `input.tsx` produces today (clean deopt
   when the resolver finds nothing).

2. **`input3.tsx` (in-cwd relative import) still folds `9999` byte-for-byte.**
   Run it before AND after your fix, diff the output, must be identical.

3. **All existing parity-harness fixtures still PARITY.**
   ```bash
   cd /Users/sjackson3/Documents/sjcompiled
   bun parity-harness/fixtures-triage.mjs
   ```
   No regressions.

4. **Cargo tests pass.**
   ```bash
   cd /Users/sjackson3/Documents/sjcompiled/crates
   cargo test -p babel-plugin
   ```

5. **No drift from upstream JS.** Per CLAUDE.md, "BUGS in OLD! Need to be
   BUGS In NEW." The Rust port's job is to behave the same as
   `packages/babel-plugin` does in the same situation. Upstream JS doesn't
   have this bug because Node has no sandboxed FS — the file read either
   succeeds or throws cleanly. The Rust port's analog of "the file read
   threw" is "the WASI read returned `Err(ENOTCAPABLE)`". The fix is to
   make sure that error path produces the same observable outcome as
   "the resolver couldn't find the import at all" (which IS upstream
   behaviour and DOES result in a clean deopt to the runtime fallback).

   **Concretely**: the right shape of fix is almost certainly to make
   `unwrap_or_default()` at `resolve_binding.rs:663` (and the equivalent
   at `evaluate_expression.rs:669`) propagate the read failure back to
   the caller as "import not foldable" (i.e. return `None`/deopt) rather
   than silently substituting an empty source string. The empty-source
   path was likely added to handle the case where a file is **legitimately
   empty** (zero bytes); make sure the fix still handles that
   distinguishably from "read failed".

   **DO NOT add an arbitrary depth-cap** to `traverse_call_expression`.
   That was the bug reporter's suggestion and it's drift from upstream
   (no such cap exists in `packages/babel-plugin/src/utils/traverse-expression/traverse-call-expression.ts`).
   It would also silently truncate legitimate deep evaluations elsewhere.

6. **Add a regression test.** A `cargo test` at the right layer that
   asserts: when `read_to_string` returns `Err` on an unreadable path,
   the binding resolution returns `None` (or whatever the deopt sentinel
   is), and the upstream `traverse_call_expression` produces a
   runtime-fallback shape, in bounded time. The test does NOT need a
   real WASI sandbox — fake the FS-read failure by stubbing whatever
   layer is convenient (look at how `state.cache.read_file` is
   constructed; if it accepts a closure, inject one that returns
   `String::new()` plus an "actually-failed" flag).

## Don't do

- Don't touch `packages/*` (immutable per CLAUDE.md).
- Don't add a depth cap or visited-set to `traverse_call_expression`
  unless you've FIRST proven (via a stack trace) that the loop is
  inside that function. The previous agent established it almost
  certainly is not — the actual loop is somewhere upstream that keeps
  re-feeding the same shape back to the call expression.
- Don't widen the `progressed` heuristic at lines 425-433 to "match
  more cases" without first confirming via instrumentation that the
  observed behaviour matches your understanding of which path is
  cycling. The structural check is in straight-line code — fixing
  it won't terminate a loop that lives elsewhere.
- Don't change the host-side harness/wrapper code as a "fix". The
  reporter's options A and B (mount a second preopen, host-resolve
  symlinks before invoking the plugin) are out of scope here — they
  may be sensible long-term improvements but are not THIS bug's fix.
  AFM-prod's host wrapper is what it is; the plugin must handle
  unreadable paths gracefully without our help.

## Useful files (1:1 ports — read both sides side by side)

| Rust | TypeScript upstream |
|---|---|
| `crates/babel-plugin/src/utils/traverse_expression/traverse_call_expression.rs` | `packages/babel-plugin/src/utils/traverse-expression/traverse-call-expression.ts` |
| `crates/babel-plugin/src/utils/traverse_expression/traverse_member_expression/mod.rs` | `packages/babel-plugin/src/utils/traverse-expression/traverse-member-expression/index.ts` |
| `crates/babel-plugin/src/utils/traverse_expression/traverse_member_expression/traverse_access_path/resolve_expression/mod.rs` | `packages/babel-plugin/src/utils/traverse-expression/traverse-member-expression/traverse-access-path/resolve-expression/index.ts` |
| `crates/babel-plugin/src/utils/evaluate_expression.rs` | `packages/babel-plugin/src/utils/evaluate-expression.ts` |
| `crates/babel-plugin/src/utils/resolve_binding.rs` | `packages/babel-plugin/src/utils/resolve-binding.ts` |
| `crates/babel-plugin/src/compat/wasi_path.rs` | (no upstream — WASI-only) |

The previous agent's chat transcript covering this investigation
includes verified findings on the previous 6 reported bugs (4 of them
were misreports caused by the monorepo team having `@compiled/css 0.20`
installed instead of the AFM-pinned `0.19.0`, 1 was a JSX-pragma host-
config issue, 1 was real). That context will help calibrate how much
to trust the bug reporter's diagnosis on this one — they correctly
identified the WASI symlink-escape angle, but their proposed fix
(depth cap in `traverse_call_expression`) is not the right shape per
CLAUDE.md. The right fix is to propagate the FS read error.

## Build & test cycle

```bash
# Rebuild the WASM plugin (2-3 min — normal):
cd /Users/sjackson3/Documents/sjcompiled
cargo build -p babel-plugin --release --target wasm32-wasip1

# Verify the hang is fixed:
cd /tmp/_rovodev_symlink_repro/jira
perl -e 'alarm 10; exec @ARGV' bun ../probe.mjs input4.tsx     # should finish <1s
perl -e 'alarm 10; exec @ARGV' bun ../probe.mjs input5.tsx     # should finish <1s
perl -e 'alarm 10; exec @ARGV' bun ../probe.mjs input6.tsx     # should finish <1s

# Verify regressions:
perl -e 'alarm 10; exec @ARGV' bun ../probe.mjs input.tsx      # 22ms clean deopt
perl -e 'alarm 10; exec @ARGV' bun ../probe.mjs input3.tsx     # 20ms folds 9999
perl -e 'alarm 10; exec @ARGV' bun ../probe.mjs input7.tsx     # clean deopt

# Full parity harness (the gate that matters):
cd /Users/sjackson3/Documents/sjcompiled
bun parity-harness/fixtures-triage.mjs
```

When everything is green, write up a one-paragraph note describing
exactly which file/line you changed and why, and which of the three
working hypotheses turned out to be correct. Then stop.
