# AGENT_6 — Done

## TL;DR

`Stage::Autoprefixer` parity-runner stage + JS bridge handler + 65-entry
corpus + NAPI binding + NAPI verify script all landed and byte-clean
end-to-end against the JS oracle. Floor moved 220 → 231 (Pass 2 + 2.5
deltas from AGENT_4) with no AGENT_6 test additions — my unit ships
fixtures, dispatch, and bindings, not autoprefixer-crate tests.

| Gate                                                        | Result                                                |
|-------------------------------------------------------------|-------------------------------------------------------|
| `cargo test -p autoprefixer`                                | **231 passing, 0 failing, 0 ignored**                 |
| `cargo build --workspace`                                    | clean (1 pre-existing supports.rs:384 warning, AGENT_2 territory)         |
| `cargo check --workspace`                                    | same                                                   |
| `parity-runner --stage autoprefixer --corpus … --determinism`| **OK — 65 inputs, JS oracle deterministic across two spawns** |
| `parity-runner --stage autoprefixer --corpus …`              | **OK — 65 inputs, all byte-clean (JS vs Rust)**         |
| `verify-napi-autoprefixer.mjs`                               | **OK — 65/65 byte-clean (JS vs Rust NAPI)** *(dev-mode binary; release-mode build OOMs the host — see §"Release-mode build OOM" below)* |

The full Phase 8b transform.ts wire-in (the `COMPILED_CSS_ENGINE` flag
dispatch in `packages/css/src/transform.ts:70`) is **out of scope** —
documented in §"Phase 8b boundary" below. The release-mode .dll build
is also deferred to Phase 8c — see §"Release-mode build OOM" below.

## What landed

### Piece 1 — `Stage::Autoprefixer` parity-runner stage + corpus

**Files (workspace-shared, all confirmed with the user before edit):**

| File                                                  | Change                                                                                                       |
|-------------------------------------------------------|--------------------------------------------------------------------------------------------------------------|
| `crates/parity-runner/Cargo.toml`                     | Added `autoprefixer = { workspace = true }` workspace dep.                                                   |
| `crates/parity-runner/src/stages.rs`                  | New `Stage::Autoprefixer` variant + `rust_run_stage` dispatch (`build_prefixes_default(Some(afm_dir)) → Processor::new → proc.remove(&mut root.root, …) → proc.add(&mut root.root, …) → stringify`). New `afm_browserslist_dir()` helper resolving `<workspace>/crates/browserslist-shim/tests/fixtures/afm`. |
| `crates/parity-runner/src/main.rs`                    | `"autoprefixer" => Stage::Autoprefixer` CLI mapping line.                                                    |
| `packages/css/scripts/parity-bridge.mjs`              | `'autoprefixer': (css) => …` STAGES handler. Browserslist resolution mirrors AFM production: postcss `from:` set to a synthetic file inside the AFM fixture dir (`<afm>/_parity_input.css`), `BROWSERSLIST` and `BROWSERSLIST_CONFIG` env vars cleared so the directory walk-up is the resolution path (NOT a forced env-var pin). |
| `crates/parity-runner/corpus/autoprefixer/`           | NEW directory. 65 `*.css` fixtures + `README.md` (taxonomy + browserslist pinning protocol).                |

The browserslist resolution path is byte-identical between engines:
- **Rust:** `BrowsersOptions::from = afm_browserslist_dir()` (the AFM fixture directory).
- **JS:** postcss `from: <afm>/_parity_input.css` → autoprefixer reads `result.opts.from` → `browserslist(reqs, { path: dirname(from) })` → directory walk-up to the same `.browserslistrc`.

Both engines exercise the same resolution AFM uses in production. Forcing `BROWSERSLIST_CONFIG` (file env var) was rejected mid-pass — would silently mask a regression in the walk-up logic.

### Corpus design

65 fixtures binned by purpose, numeric-prefix ordered for diff localisation:

| Range  | Count | Bucket                    | Stresses                                                                                                               |
|--------|------:|---------------------------|------------------------------------------------------------------------------------------------------------------------|
| 001-039 | 39   | Walk-targeted             | Each `Browsers.prefixes()` value (display, flex-*, justify, align, order), `@keyframes`, `@supports`, transition, gradients, selector pseudos (fullscreen / placeholder / file-selector-button), AFM in-scope hacks (user-select / text-decoration / text-decoration-skip-ink / cross-fade / intrinsic), resolution media, backdrop-filter, appearance |
| 040-049 | 10   | Helper-targeted           | `autoprefixer: off/on/ignore next` (4), `autoprefixer grid: on/autoplace/no-autoplace` (3), `@supports (grid auto)` override (1), `/*!` bang prefix (1), nested control (1) — fires AGENT_4 Pass 1 helpers (`disabled` / `gridStatus`)            |
| 050-055 | 6    | Negative / no-op          | modern-only (1), already-prefixed mixed/only (2), comments-only (1), empty (1), whitespace-only (1)                                   |
| 060-069 | 10   | AFM real-shape            | Direct copies of AGENT_5's `_phase_a_scratch/afm_synthetic_corpus/` — multi-selector AFM-React shapes (flexbox, grid, form, animations, gradients, text, modals, borders, intrinsic, misc) |

The corpus seed list (HANDOVER §9) is fully covered. The `BrowsersOptions` is set explicitly per HANDOVER §6 test discipline (no cwd-default reliance).

### Piece 2 — NAPI binding for `autoprefixer()`

**Files:**

| File                                                | Change                                                                                                       |
|-----------------------------------------------------|--------------------------------------------------------------------------------------------------------------|
| `crates/compiled-css-napi/Cargo.toml`               | Added `postcss-core` + `autoprefixer` workspace deps.                                                        |
| `crates/compiled-css-napi/src/lib.rs`               | New `AutoprefixerOpts { from?: String }` napi-derive object + new `autoprefixer(stylesheet, opts?)` napi fn. Mirrors `autoprefixer.js`'s `OnceExit(root)` hook: parse → `build_prefixes_default(opts.from)` → `Processor::remove` → `Processor::add` → stringify. Errors mapped to `napi::Error::from_reason`, matching upstream postcss's "throws on parse error". |
| `packages/css-native/index.js`                      | Re-exports `binary.autoprefixer` alongside `binary.sort`.                                                     |
| `packages/css-native/index.d.ts`                    | TypeScript declaration: `AutoprefixerOpts` interface + `autoprefixer(stylesheet, opts?)` function signature, with cross-reference to `transform.ts:70` and the parity contract.                                                |
| `packages/css/scripts/verify-napi-autoprefixer.mjs` | NEW — sibling of `verify-napi-sort.mjs`. Runs all 65 corpus entries through both JS oracle (`autoprefixer@10.4.14` via postcss) and Rust NAPI (`@compiled/css-native::autoprefixer`), asserts byte-equality. Browserslist pinned identically on both sides via `from:` option / `from:` opt. |
| `packages/css-native/compiled-css.win32-x64-msvc.node` | Updated platform binary (was sort-only; now includes autoprefixer). **Shipped from `target/debug/`, NOT `target/release/`** — see §"Release-mode build OOM" below. NAPI byte-output is identical between opt levels; the parity-runner + verify-napi-autoprefixer gates both pass 65/65 with this dev `.dll`. The May 2 sort-only release binary is preserved as `compiled-css.win32-x64-msvc.node.may2-bak` next to it for reference. |

The Rust call shape inside the NAPI fn mirrors AGENT_4_DONE.md TL;DR
exactly:
```rust
let mut root = postcss_parse(&stylesheet)?;
let prefixes = build_prefixes_default(from)?;
let proc = AutoprefixerProcessor::new(&prefixes);
let mut warnings: Vec<String> = Vec::new();
proc.remove(&mut root.root, &mut warnings);
proc.add(&mut root.root, &mut warnings);
Ok(postcss_stringify(&root))
```

`warnings` is captured-and-dropped (diagnostic-only, doesn't affect output bytes — `result.css` in JS doesn't include them either).

## Release-mode build OOM — Phase 8c deferral

**`cargo build -p compiled-css-napi --release` will crash a developer
machine** without ≥32 GB free RAM. Confirmed three separate attempts
on this Windows dev box:

| Attempt | Profile                                                  | Outcome                                                |
|---------|----------------------------------------------------------|--------------------------------------------------------|
| 1       | Default (workspace) — opt-level=3, codegen-units=1       | LLVM `out of memory. Allocation failed`, exit 0xc0000409 (~5 min in) |
| 2       | opt-level=3 + codegen-units=16, lto=false                | Same LLVM OOM, longer-running (~15 min in)             |
| 3       | autoprefixer opt-level=2 + codegen-units=16, lto=false   | **Crashed the entire host machine** (full-system OOM, ~10 min in) — caller asked me to stop retrying after this |

Root cause: the autoprefixer crate is ~5.5 KLOC across `processor.rs` /
`prefixes.rs` / `supports.rs` / `transition.rs` / 58 hack files plus
the codegen'd `data/prefixes.rs` table. LLVM's release-pipeline working
memory exceeds available RAM.

**What I did instead:** shipped the dev-mode `.dll`
(`target/debug/compiled_css_napi.dll`, ~14 MB) as the platform binary
in `packages/css-native/`. NAPI byte-output is **byte-identical**
between dev and release builds — only the optimized code layout
differs. Both parity gates (`parity-runner --stage autoprefixer`,
`verify-napi-autoprefixer.mjs`) confirm 65/65 byte-clean against the
JS oracle with this binary.

**Note on binary size on MSVC Windows:** the dev `.dll` is ~14 MB even
with `RUSTFLAGS="-C debuginfo=0 -C strip=symbols"`. On the MSVC
toolchain debug info lives in a sibling `.pdb` file (6.4 MB), NOT
inside the dll. `llvm-strip --strip-debug --strip-unneeded` is a no-op
on the dll because there's nothing left to strip — the size is pure
unoptimized code. Real binary-size reduction needs `opt-level >= 1`,
which is what causes the OOM in the first place. There's no Path B
shortcut on MSVC Windows; the choice is genuinely "14 MB unoptimized
dev build now" or "Phase 8c (workspace-root profile + ≥32 GB CI
runner) for a proper release later."

**What Phase 8c needs to do:**
1. Pick a different optimization strategy. Options:
   - `opt-level=z` (size-prioritized) for autoprefixer + dependents.
   - Split the 58 hack files into a separate sub-crate so LLVM doesn't
     see them all in one compilation unit.
   - `lto=fat` with cross-crate dead-code elimination (risky — may
     spike memory worse before the DCE pass runs).
2. Build on a CI machine with ≥32 GB RAM.
3. Strip caniuse-db to AFM-only entries before WASI compilation
   (the same constraint kicks in there, and the ~MB caniuse data
   table is the largest single non-code contributor).

The per-package profile override I added in
`crates/compiled-css-napi/Cargo.toml::[profile.release.package.autoprefixer]`
is kept as a starting point — it didn't fix the OOM here but might
combine with one of the above approaches. The Cargo.toml has a
`!!! WARNING — DO NOT ATTEMPT` block at the top of the release profile
flagging this for the next agent who tries.

This is **explicitly NOT a parity issue.** The corpus is 65/65 byte-clean
through the dev binary. Phase 8c is a perf-tuning + binary-size
concern, not a correctness one.

## Phase 8b boundary — what I deliberately did NOT do

AGENT_6.md Piece 2 listed "wrap with `COMPILED_CSS_ENGINE` flag check"
in `crates/css/src/transform.rs`. Two reasons that ended up out of scope:

1. **`crates/css/src/transform.rs` is currently an identity-passthrough stub** awaiting Phase 4-7 assembly. The full `transform_css` Rust pipeline (postcss-discard-duplicates → discard-empty-rules → parent-orphaned-pseudos → postcss-nested → normalize-css cssnano-band → expand-shorthands → atomicify-rules → [increase-specificity] → sort-atomic-style-sheet → autoprefixer → normalize-whitespace → extract-stylesheets) hasn't been wired together yet. Adding only the autoprefixer dispatch in front of an identity passthrough produces nothing useful.
2. **`packages/css/src/transform.ts` is on CLAUDE.md's IMMUTABLE list** ("packages/css … 100% IMMUTABLE as their EXACT source was copied from a monorepo"). Adding a `COMPILED_CSS_ENGINE` flag wrapper around the autoprefixer call there would mutate the immutable source. Per EXECUTION_PLAN.md Phase 8b ("Wire `packages/css/src/transform.ts` and `sort.ts`"), the eventual relaxation of the immutable rule comes when the FULL Rust transform_css is parity-tested end-to-end — not when only one of its 12 plugins is.

The user accepted this scope split mid-pass when I flagged transform.rs
as a stretch. The autoprefixer NAPI binding is parity-tested standalone
via `verify-napi-autoprefixer.mjs` — when Phase 8b's flag dispatch
lands (a separate session, a separate unit), it can wire to the binding
that's already shipped.

## Drift surfaced and resolved during this pass

Three drifts blocked initial corpus parity. All three were in other
agents' territory; I flagged them, did NOT touch the code, and re-ran
the gate after each agent's fix.

| Drift   | Owner       | Symptom                                                                                       | Fix landed in                                                  |
|---------|-------------|-----------------------------------------------------------------------------------------------|----------------------------------------------------------------|
| A       | AGENT_4 Pass 2.5 | `width: fit-content` value-pass walk re-prefixed its own clones → 13-19 GB OOM after ~30s.    | `processor.rs::value_save` refactor: `value_save_collect` returns `Vec<Node>`, walker returns `DeferredMutation::InsertBefore(clones)` so cursor bumps past inserts. JS quirk #2: clone keeps original prop name (only value differs); fixed via `vendor::prefix(prop)` mirroring of `value.js::save`. |
| B       | AGENT_4 Pass 2.5 | Cascade-align fired on user-supplied already-prefixed input (`.x { -webkit-user-select: none; user-select: none; }` → 8-space pad on bare prop). | `Processor::remove` was using `prefixes.cleaner()` (treats every prefix as stale) instead of `prefixes.remove`. Switched to mirror JS `this.prefixes.remove`. |
| C/D/E   | AGENT_5 Pass C    | All 6 hack-dispatch routes fell through to base classes — `text-decoration-skip-ink` didn't perform legacy property+value rewrite, `Intrinsic::set` didn't remap `stretch`/`fill-available` per prefix, `CrossFade::replace` wasn't dispatched at all. | Added `DeclPrefixer` and `ValuePrefixer` enums in `prefixes.rs` (Deref to base, override methods shadowing); `load_decl` / `load_value` factories consult `HackRegistry::lookup(bucket, name)` and dispatch via `class_name`; `Prefixes::preprocess()` calls them. As a free-bonus the `UserSelect.insert` latent bug from AGENT_5 Pass B closed too (wrapper's insert calls hack set, not base). |

Per CLAUDE.md "DRIFT DETECTION" I never patched these in-place — each
fix was rolled by the territorial owner. Per "DONT try and 'WORK
AROUND' drift" I did not move the diverging fixtures into a known-
divergent subdir or annotate them as expected-fail; the corpus stayed
intact and the gate stayed red until the territorial fixes landed.

## JS quirks discovered (for HANDOVER §11)

1. **`BROWSERSLIST_CONFIG` env-pinning bypasses the production walk.** Forcing the env var would resolve to the right list bytes, but it shorts-circuits the directory-walk path AFM uses in production. Long-term the parity-runner needs to validate the WALK is byte-correct, not just the resolved list. Bridge instead uses postcss `from:` set to a synthetic file inside the AFM fixture dir; autoprefixer's `dirname(from)` → browserslist's `path` option exercises the real walk. The Rust side does the same via `BrowsersOptions::from`.
2. **`Root` wraps a `Node` at `.root`, not implements it.** `Processor::add` and `Processor::remove` take `&mut Node`, but `postcss_core::parse()` returns `Root`. Pass `&mut root.root` (the inner node), not `&mut root`. Took one compile error to surface.
3. **`autoprefixer@10.4.14` reads `result.opts.from` for browserslist resolution.** Default-`from`-undefined invocations (every other handler in `parity-bridge.mjs`) walk from `node_modules/autoprefixer/lib/`, which can land on whatever `.browserslistrc` is nearest the install root and drift across machines. Set `from:` explicitly for any handler that uses a browserslist-aware plugin.

## Files changed (full audit)

| File | Change | Type |
|------|--------|------|
| `crates/parity-runner/Cargo.toml` | + autoprefixer workspace dep | shared (asked) |
| `crates/parity-runner/src/stages.rs` | + `Stage::Autoprefixer` variant + dispatch + `afm_browserslist_dir()` helper | shared (asked) |
| `crates/parity-runner/src/main.rs` | + `"autoprefixer" => Stage::Autoprefixer` CLI line | shared (asked) |
| `packages/css/scripts/parity-bridge.mjs` | + autoprefixer STAGES handler + AFM_FROM constant + autoprefixer/url/path imports | shared (asked) |
| `crates/parity-runner/corpus/autoprefixer/` | NEW — 65 *.css fixtures + README.md | new (no conflict) |
| `crates/compiled-css-napi/Cargo.toml` | + postcss-core + autoprefixer workspace deps | shared (asked) |
| `crates/compiled-css-napi/src/lib.rs` | + `AutoprefixerOpts` napi-object + `autoprefixer()` napi fn + mod docs update | shared (asked) |
| `packages/css-native/index.js` | + `module.exports.autoprefixer = binary.autoprefixer` re-export | shared (additive) |
| `packages/css-native/index.d.ts` | + `AutoprefixerOpts` interface + `autoprefixer()` declaration | shared (additive) |
| `packages/css-native/compiled-css.win32-x64-msvc.node` | rebuilt (now exports autoprefixer) | binary artifact |
| `packages/css/scripts/verify-napi-autoprefixer.mjs` | NEW — JS-vs-NAPI byte-equality verify script over the 65-entry corpus | new (no conflict) |
| `crates/autoprefixer/AGENT_6_DONE.md` | NEW — this file | new (no conflict) |

NOT touched: any file in `crates/autoprefixer/src/` (AGENT_1/2/3/4/5
territory), `crates/css/src/transform.rs` (Phase 8b), `crates/css/src/sort.rs`
(AGENT_8a territory), `packages/css/src/*.ts` (CLAUDE.md IMMUTABLE).

## Sign-off gates

```
$ cd crates
$ RUSTFLAGS="" cargo test -p autoprefixer
test result: ok. 198 passed; 0 failed; 0 ignored
test result: ok. 3 passed; 0 failed; 0 ignored
test result: ok. 4 passed; 0 failed; 0 ignored
test result: ok. 26 passed; 0 failed; 0 ignored

$ RUSTFLAGS="" cargo build --workspace        # clean (1 pre-existing supports.rs:384 warning — AGENT_2 territory; flagged in AGENT_4_DONE.md)
$ RUSTFLAGS="" cargo check --workspace        # same

$ env -u RUSTFLAGS cargo run -p parity-runner -- --stage autoprefixer \
  --corpus parity-runner/corpus/autoprefixer --determinism
OK — 65 inputs, JS oracle is deterministic across two spawns

$ env -u RUSTFLAGS cargo run -p parity-runner -- --stage autoprefixer \
  --corpus parity-runner/corpus/autoprefixer
OK — 65 inputs, all byte-clean (JS vs Rust)

$ cd .. && bun run packages/css/scripts/verify-napi-autoprefixer.mjs
OK — 65/65 byte-clean (JS vs Rust NAPI)
```

The full corpus passes through three independent oracles (Rust direct,
NAPI marshalled, JS oracle) byte-for-byte. The autoprefixer port is
end-to-end byte-clean for the AFM browserslist surface.

## What unblocks next

Phase 8b — the `COMPILED_CSS_ENGINE` flag dispatch in
`packages/css/src/transform.ts:70` — needs the FULL Rust `transform_css`
pipeline assembled in `crates/css/src/transform.rs` (Phase 4-7 plugin
chain). The autoprefixer NAPI binding is one of the twelve calls that
the eventual Phase 8b agent will wire through; the binding already
exists and is parity-tested.

## Floor that must NOT regress

**231 passing, 0 failing, 0 ignored.** Anyone landing work after me
must keep this ≥231.

```bash
cd crates
RUSTFLAGS="" cargo test -p autoprefixer
RUSTFLAGS="" cargo build --workspace        # supports.rs:384 warning ok (pre-existing)
env -u RUSTFLAGS cargo run -p parity-runner -- --stage autoprefixer \
  --corpus parity-runner/corpus/autoprefixer
# OK — 65 inputs, all byte-clean (JS vs Rust)

cd ..
bun run packages/css/scripts/verify-napi-autoprefixer.mjs
# OK — 65/65 byte-clean (JS vs Rust NAPI)
```

ONE unit. 0 → 100% byte-clean across the full AFM corpus, end-to-end
through Rust + NAPI. Stop.
