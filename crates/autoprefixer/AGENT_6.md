# AGENT_6 — `Stage::Autoprefixer` parity-runner + NAPI wire-in

You are picking up the FINAL unit inside the larger `crates/autoprefixer`
port — wiring it into the rest of the system. You have NO memory of the
prior conversation — this file plus the docs it points at are your full
briefing.

---

## What you own

ONE unit, taken 0 → 100% byte-clean against the JS oracle, in two
sub-pieces:

### Piece 1: `Stage::Autoprefixer` parity-runner stage + corpus

**Files you'll touch (ALL workspace-shared — see "Pre-flight" below):**
- `crates/parity-runner/src/stages.rs` — add `Stage::Autoprefixer`
  variant + dispatch.
- `crates/parity-runner/src/main.rs` — CLI flag mapping.
- `packages/css/scripts/parity-bridge.mjs` — JS oracle counterpart.
- `crates/parity-runner/corpus/autoprefixer/` — NEW directory of
  CSS test inputs (~30–50 entries per HANDOVER §9 corpus seed list).

The runner spawns the JS oracle (real `autoprefixer@10.4.14` via bun
+ AFM-pinned `browserslist@4.24.2`) AND the Rust port on the same
input, byte-compares, fails on any divergence. This is the gate that
proves end-to-end parity.

### Piece 2: NAPI wire-in to `crates/css/src/transform.rs`

**File you'll touch (workspace-shared):**
- `crates/css/src/transform.rs` — call into Rust autoprefixer behind
  the existing `COMPILED_CSS_ENGINE` flag. The JS pipeline stays as the
  parity oracle and emergency fallback (per `EXECUTION_PLAN.md` Phase 8b
  and cross-cutting policy #10 — "do not delete the JS pipeline").

---

## Pre-flight: ASK THE USER FIRST

The four files above are explicitly "ask first" workspace-shared per
`HANDOVER.md` §8 and `MORNING.md`. Before editing ANY of them, post
to the user:

> "I'm AGENT_6, ready to wire the autoprefixer parity-runner stage and
> NAPI bridge. This requires editing
> `crates/parity-runner/src/{stages,main}.rs`,
> `packages/css/scripts/parity-bridge.mjs`, and
> `crates/css/src/transform.rs`. Confirm I can proceed, and confirm no
> parallel agent has changes queued for any of these files."

DO NOT EDIT WITHOUT EXPLICIT CONFIRMATION. The cost of a merge conflict
on `transform.rs` is a stalled session for whoever owns it elsewhere.

---

## Hard pre-conditions — do not start without these green

1. **AGENT_1, AGENT_2, AGENT_3, AGENT_4 have all landed.** That means
   `Prefixes::new`, `Supports`, `Transition`, and `processor.rs` are
   real. Without them, the parity-runner stage will dispatch into
   `unimplemented!()` and panic.
2. **AGENT_5's Phase B has landed for the AFM hack subset.** Without
   the hacks AFM uses, the parity-runner stage will produce CSS that's
   missing 40% of real-world prefix output (per HANDOVER §9). Byte-test
   against JS will fail across the board.
3. The 6 sign-off gates from the previous 5 agents are all green:
   ```bash
   cd crates
   RUSTFLAGS="" cargo test -p autoprefixer    # ≥60 + everyone's adds, 0 failing, 0 ignored
   RUSTFLAGS="" cargo build -p autoprefixer   # clean
   RUSTFLAGS="" cargo check --workspace       # clean
   ```

If any pre-condition fails: STOP. Don't proceed. Document which
pre-conditions are unmet and exit.

---

## Read these BEFORE writing code (in this order)

1. `crates/PARITY_VERSIONS.md` — byte-equality contract. ~5 min.
2. `crates/EXECUTION_PLAN.md` — read Phase 7 (autoprefixer exit gates) +
   Phase 8 (bridge & assembly) end-to-end. ~10 min.
3. `crates/PLUGIN_IMPLEMENTATION_GUIDE.md` — AST surface, helpers.
   ~5 min.
4. `crates/autoprefixer/HANDOVER.md` — read all of it. ~20 min. Especially:
   - §1 (current floor)
   - §6 (browserslist gate, test discipline — your end-to-end tests
     MUST set `BrowsersOptions::from` explicitly to AFM fixture path)
   - §8 (workspace-shared files protocol)
   - §9 (corpus seed list — your starting point for
     `crates/parity-runner/corpus/autoprefixer/`)
5. `crates/autoprefixer/GATE_CLOSED_FOR_AUTOPREFIXER_AGENT.md` ~5 min.
6. AGENT_1_DONE.md, AGENT_2_DONE.md, AGENT_3_DONE.md, AGENT_4_DONE.md,
   AGENT_5_DONE.md — confirm what landed. ~10 min.
7. **The existing parity-runner code:**
   `crates/parity-runner/src/stages.rs`, `main.rs`, and any existing
   stage (`Stage::Sort`, `Stage::Cssnano*`, etc.) for the pattern you'll
   follow. ~15 min.
8. **The JS bridge counterpart:** `packages/css/scripts/parity-bridge.mjs`
   for the existing per-stage handlers. ~10 min.
9. **The existing `crates/css/src/transform.rs`** for how
   `COMPILED_CSS_ENGINE` is dispatched today. ~10 min.
10. The cssnano agents' parity-runner stages — they're the pattern your
    autoprefixer stage should mirror exactly (same shape, same
    error handling, same diff output). ~15 min.

Total: ~100 min reading.

---

## Where things stand at start

`cargo test -p autoprefixer` → **whatever the prior 5 agents left it
at**. Floor.

What's REAL when you start:
- Everything autoprefixer has: base classes, `Prefixes`, `Supports`,
  `Transition`, `processor.rs`, hack subset AFM uses.
- The parity-runner has stages for `sort` (done — Phase 8a) and at
  least scaffolded stages for cssnano plugins.
- The JS bridge `parity-bridge.mjs` has handlers for those stages.
- `crates/css/src/transform.rs` has the `COMPILED_CSS_ENGINE` flag
  with a `sort` Rust path live; transform path likely still routes to
  JS unconditionally (per Phase 8b "not started" status).

---

## Sub-piece 1: parity-runner stage + corpus design

### Stage shape

Add `Stage::Autoprefixer` variant. Its handler:
1. Reads a `(input.css, browserslist_path)` pair from a corpus entry.
2. Runs Rust:
   ```rust
   let opts = BrowsersOptions {
       from: Some(browserslist_path.to_string_lossy().into_owned()),
       ..Default::default()
   };
   let browsers = Browsers::new("", opts);
   let prefixes = Prefixes::new(browsers, &PREFIXES);
   let mut root = postcss_core::parse(input)?;
   processor::process(&mut root, &prefixes)?;
   let rust_output = postcss_core::stringify(&root);
   ```
3. Spawns bun against the JS oracle bridge:
   ```bash
   bun packages/css/scripts/parity-bridge.mjs --stage autoprefixer \
     --input <path> --browserslist <fixture-dir>
   ```
4. Byte-compares both. On divergence, emit smallest divergent byte
   range with surrounding context (mirror what other stages do).

### Corpus seed (per HANDOVER §9)

Start with ~30–50 inputs:
- Each `Browsers.prefixes()` value gets one fixture exercising it.
- `display: flex`, `display: grid` — most consequential decls.
- `@keyframes` (at-rule prefix path).
- `@supports (...)` (selector inside is a known wrinkle — pinned by
  AGENT_2's tests).
- `transition: transform 0.3s` (AGENT_3's territory — exercises full
  pipeline integration).
- `linear-gradient(...)` (gradient hack — AGENT_5 must have ported it
  if AFM uses it).
- `:fullscreen`, `::placeholder`, `::file-selector-button` (selector
  hacks — AGENT_5 if AFM uses them).
- A "no-op" fixture per browser query (e.g. `last 1 chrome version` →
  no prefixing happens) — proves the negative case.
- An input that mixes already-prefixed + unprefixed in the same rule
  (catches `isAlready` + `otherPrefixes` interaction).
- **Real AFM CSS samples** — coordinate with the user to capture a
  representative slice of CSS from the actual AFM build. The synthetic
  corpus catches known-shape bugs; the real corpus catches the unknown
  ones.

Each corpus entry is a tuple. File layout (mirror existing stages):
```
crates/parity-runner/corpus/autoprefixer/
  001-display-flex/
    input.css
    browserslist.path           # absolute path or relative-to-workspace
  002-supports-display-flex/
    input.css
    browserslist.path
  ...
```

The browserslist path defaults to the AFM fixture (most cases).
Negative-case entries can pin a different `.browserslistrc`.

### JS bridge

`packages/css/scripts/parity-bridge.mjs --stage autoprefixer`:
1. Reads the same input.
2. Sets `process.cwd()` (or `path` opt) to the browserslist fixture
   dir.
3. Calls `require('autoprefixer')()` — the real upstream JS
   (workspace's pinned `autoprefixer@10.4.14`).
4. Runs through `postcss(...).process(input).css`.
5. Writes result to stdout for the runner to compare.

Mirror the existing per-stage bridge handlers exactly. Forgetting the
JS bridge side produces "no diff" output that LOOKS green because both
sides hit the unknown-stage error path (per HANDOVER §8 footnote).

### Exit gate

`cargo run -p parity-runner -- --stage autoprefixer` → 100% byte-clean
across the entire corpus. Any divergence is a hash-rotation event;
debug, fix, or document as a known divergence (e.g., a hack AFM doesn't
use that we deliberately stubbed).

---

## Sub-piece 2: NAPI wire-in

Per `EXECUTION_PLAN.md` Phase 8b:

1. Extend `crates/compiled-css-napi/` to expose
   `autoprefixer(css: string, opts: AutoprefixerOpts) -> string`.
   `AutoprefixerOpts` mirrors what AFM passes:
   `{ from?: string, browsers?: string[] }`.
2. In `crates/css/src/transform.rs`, find where autoprefixer is called
   in the JS pipeline. Wrap with the `COMPILED_CSS_ENGINE` flag check:
   - `js` (default) → existing JS pipeline unchanged.
   - `rust` → call into `@sjcompiled/css-native`'s `autoprefixer`.
3. **Do not delete the JS pipeline.** It stays as the parity oracle and
   emergency fallback per cross-cutting policy #10.
4. Build platform binaries — workspace already does this for `sort()`,
   mirror that build config.

### Exit gate

The Phase 0 harness runs both engines under the flag and gets 0 bytes
diff across the full corpus. `transformCss` is byte-clean end-to-end
through Rust autoprefixer.

---

## Test discipline — DO NOT rely on cwd

Per HANDOVER §6: every Rust test that uses `Browsers::new(...)` MUST
set `BrowsersOptions::from` explicitly. The parity-runner stage handler
DOES the equivalent (the `browserslist_path` corpus field). Do not let
that field be optional with a cwd-default — make it required per corpus
entry to remove ambiguity.

Generic queries (`defaults` / `> 1%`) drift through oxc fallback.
Don't put corpus entries that depend on those unless you mark them
known-divergent.

---

## What you must NOT do

1. Do NOT touch `prefixes.rs`, `supports.rs`, `transition.rs`,
   `processor.rs`, or any `hacks/*.rs`. The other agents own those.
   If your end-to-end runs surface bugs in those files, FILE THEM as
   notes for the appropriate agent — do not fix in your session.
2. Do NOT bump any pinned version.
3. Do NOT "fix" upstream bugs. Replicate.
4. Do NOT use `HashMap`. `IndexMap` only.
5. Do NOT remove `oxc_browserslist` or widen the AFM grammar.
6. **Do NOT delete `crates/css/src/transform.ts`'s JS pipeline.** Per
   cross-cutting policy #10, it stays as the parity oracle for at
   least 12 months post-rollout (Phase 10d).
7. Do NOT enable Rust as the default `COMPILED_CSS_ENGINE` value.
   Default stays `js`. Flip happens in Phase 10c after weeks of clean
   shadow signal.
8. Do NOT skip the bridge counterpart in `parity-bridge.mjs`. Forgetting
   it makes the runner falsely report green.

---

## Sign-off gates — run all three before claiming done

```bash
cd crates
RUSTFLAGS="" cargo test -p autoprefixer        # must show ≥(prior floor) passing, 0 failing, 0 ignored
RUSTFLAGS="" cargo build --workspace           # clean (NAPI included)
RUSTFLAGS="" cargo check --workspace           # clean

# Parity-runner gate:
cargo run -p parity-runner -- --stage autoprefixer --corpus crates/parity-runner/corpus/autoprefixer
# must report 0 byte diffs across the entire corpus

# JS bridge sanity:
cd packages/css
bun run scripts/parity-bridge.mjs --stage autoprefixer --input crates/parity-runner/corpus/autoprefixer/001-display-flex/input.css --browserslist crates/browserslist-shim/tests/fixtures/afm
# should print prefixed CSS to stdout (not error)
```

If anything fails and you can't fix in 10 min on a workspace-shared
file, ROLL BACK and document.

---

## What to write when done

Write `crates/autoprefixer/AGENT_6_DONE.md` with:
- Test count delta + parity-runner corpus size + 0-byte-diff
  confirmation.
- File-by-file summary of every workspace-shared file you edited.
- The list of corpus entries (or the corpus index file's path).
- Any divergences surfaced and how they were resolved (fix landed
  in <agent>'s file via PR / known-divergent corpus entry / etc.).
- The `COMPILED_CSS_ENGINE=rust` invocation that proves end-to-end
  byte-clean transform.
- Confirm NAPI bridge produces byte-identical output between
  `COMPILED_CSS_ENGINE=js` and `COMPILED_CSS_ENGINE=rust` for the
  full corpus.

Do NOT update HANDOVER.md / MORNING.md / STATUS.md yourself. The
controller agent will roll up everyone's `AGENT_X_DONE.md` reports
into the central docs.

---

## If you're stuck

Vendored JS at `crates/_vendor/autoprefixer-10.4.14/package/`. The
upstream `index.js` is the entry the JS oracle bridge calls.

For parity-runner shape, read the existing `Stage::Sort` end-to-end
and mirror it.

For NAPI bridge shape, read `crates/compiled-css-napi/`'s existing
`sort()` exposure.

For `transform.rs` flag-dispatch shape, read how `sort.ts` is wired
today.

ONE unit. 0 → 100%. The destination — autoprefixer end-to-end byte-clean
under `COMPILED_CSS_ENGINE=rust`. Stop.
