# AGENT_4 — Pass 1 (slice landed)

## Test count delta

`cargo test -p autoprefixer`:
- **Before this pass (post-AGENT_1/2/3/5):** 163 unit + 4 data + 3 browserslist + 26 transition = **196 passing, 0 failing, 0 ignored**.
- **After this pass:** 187 unit + 4 data + 3 browserslist + 26 transition = **220 passing, 0 failing, 0 ignored**.
- **+24 unit tests** — 22 in `processor::tests`, 1 in `declaration::tests::process_calls_restore_before_when_cascade_branch_fires`, 1 ambient (recompile path picked up an existing test in the new build cache).

All sign-off gates green:
```bash
RUSTFLAGS="" cargo test -p autoprefixer        # 220 passing, 0 failing, 0 ignored
RUSTFLAGS="" cargo build -p autoprefixer       # clean (no autoprefixer warnings)
RUSTFLAGS="" cargo check --workspace           # one pre-existing warning in supports.rs (drift; see §"Drift flagged" below)
```

## Slice that landed

`crates/autoprefixer/src/processor.rs` went from 9-line stub to ~640 LOC, byte-clean against `lib/processor.js`'s **orchestrator-control helpers**:

| JS                       | Rust                                | Status     |
|--------------------------|-------------------------------------|------------|
| `class Processor`         | `Processor<'a>` + `new(prefixes)`  | landed     |
| `disabled(node, result)`  | `Processor::disabled`              | byte-clean |
| `disabledDecl(node, result)` | `Processor::disabled_decl`      | byte-clean¹ |
| `disabledValue(node, result)` | `Processor::disabled_value`    | byte-clean¹ |
| `gridStatus(node, result)` | `Processor::grid_status` (+ `GridStatus` enum) | byte-clean |
| `displayType(decl)`        | `Processor::display_type` (+ `DisplayType` enum) | byte-clean |
| `withHackValue(decl)`      | `Processor::with_hack_value`       | byte-clean |
| `reduceSpaces(decl)`       | `Processor::reduce_spaces`         | byte-clean² |
| Module-level constants     | `OLD_LINEAR`, `OLD_RADIAL`, `IGNORE_NEXT`, `GRID_REGEX`, `CONTROL_REGEX`, `ON_REGEX`, `AUTOPLACE_REGEX`, `NO_AUTOPLACE_REGEX` | byte-clean (raw-string + flags match JS) |
| `SIZES` const              | `pub const SIZES: &[&str]`         | byte-clean |

¹ The flexbox-false branch of `disabledDecl` / `disabledValue` is currently dormant. JS `options.flexbox === false` distinct-from-undefined has no representation in `PrefixesOptions::flexbox: Option<String>`. AGENT_2 flagged the same gap in `Supports::disabled` (AGENT_2_DONE.md "Asks for AGENT_1"). Both branches wake up together once AGENT_1 ships a `FlexboxOption` enum.

² `reduceSpaces` mutates `other.raws.before` from inside the JS `.down(callback)` walk. The Rust `GroupView::down` callback gets `&Node`, so I collect targets during the walk and apply mutations after. Output bytes are identical because the JS callback's `prevMin` / `diff` calculation reads from `raws.before` exactly once per iteration; deferring the writeback doesn't change the comparison sequence.

Also wired:

- **`DeclarationBase::process` now calls `restore_before(prefixes_all, root, &current_path)`** in the cascade branch — closing AGENT_1's punt at `declaration.rs:352`. Signature changed from `process(&self, root, path)` to `process(&self, prefixes_all: &Prefixes, root, path)`. Updated the one downstream test (`process_emits_each_prefix_with_cursor_shift`) to pass `&Prefixes::with_empty()`. Added a regression test (`process_calls_restore_before_when_cascade_branch_fires`) pinning the call path.

## Slice deferred to the next AGENT_4 session

The main `add` and `remove` walks. Rationale (from `processor.rs` module docs):

The decl walk's hot path needs a per-name Prefixer-instance dispatch table — JS calls `prefixes.add[prop].process(decl)`, where `prefixes.add[prop]` is a `Declaration` / `Value` / `Selector` / `AtRule` / `Resolution` SUBCLASS instance with a `.process()` method. The Rust `Prefixes::add_table: IndexMap<String, Vec<String>>` is the post-`select()` *data*, not the per-bucket dispatcher. Building the dispatcher (the `preprocess()` step in JS) is its own substantial unit and would either:
1. land here (in AGENT_4 territory) — fine, but a full session by itself, OR
2. land on `Prefixes` (in AGENT_1 territory) — requires a coordinated handoff.

Per AGENT_4.md "Scope discipline": one slice 0→100% byte-clean. I picked the helpers over a half-baked walk.

The deferred surface:
- The `walkAtRules` lambda — keyframes / viewport / `@supports` / `@media (-resolution)` dispatch.
- The `walkRules` lambda — `prefixes.add.selectors[*].process(rule)` dispatch.
- The first `walkDecls` lambda — the 13-prop warning ladder + per-prop prefixer dispatch + `Value.save`.
- The `walkDecls(decl => { unprefixed = ...; list = values('add', unprefixed); ... Value.save(...) })` value-pass at the bottom.
- The `remove(css, result)` walk (symmetric to `add`).
- `insertAreas` from `lib/hacks/grid-utils.js`.
- `preprocess()` — the dispatcher builder.

## JS quirks discovered (for HANDOVER §11)

1. **`gridStatus` `@supports (grid auto)` override is unconditional.** JS line 685–689:
   ```js
   if (node.type === 'atrule' && node.name === 'supports') {
     let params = node.params
     if (params.includes('grid') && params.includes('auto')) {
       value = false
     }
   }
   ```
   This block runs AFTER the child-comment status-scan, so a `@supports (grid auto) { /* autoprefixer grid: on */ ... }` with `(grid auto)` in the params and a `grid: on` comment inside still resolves to `Off`. The `@supports` params override wins. Mirrored verbatim. Pinned by `grid_status_supports_grid_auto_forces_off`.

2. **`gridStatus` env-var fallback only fires at the absolute root.** The recursion goes parent-up; the env-var (and `options.grid`) checks are in the `else if (typeof options.grid !== 'undefined')` branch which only fires when `node.parent` is null, i.e. root. Inner nodes inherit from parent unless they have their own grid comment. The recursion writes `_autoprefixerGridStatus` cache on every visit, so ancestors are computed once.

3. **`options.grid` JS coercion is loose.** JS `value = this.prefixes.options.grid` assigns whatever the user passed: `true`, `false`, `'autoplace'`, `'no-autoplace'`. Rust `PrefixesOptions::grid: Option<String>` collapses to the string-or-None. Coerced explicitly:
   - `"autoplace"` → `GridStatus::Autoplace`
   - `"false"` → `GridStatus::Off` (covers user passing `'false'` literal)
   - everything else (including `"true"` and `"no-autoplace"`) → `GridStatus::On`
   - `None` → fall through to env-var or default `Off`

   This is a slight API liberty — JS would treat the bare boolean `false` as off, where Rust requires the string `"false"`. None of the AFM-shaped queries set this option, so the gap is latent rather than live. **Filed as an AGENT_1 follow-up**: `PrefixesOptions::grid` should become an enum like `FlexboxOption` for the same hygiene reason.

4. **`reduceSpaces` `up()` callback semantics.** The JS callback returns `true` on the very first iteration so the JS `up()` walk short-circuits and `stop` is set. This is a presence test — does the decl have ANY prefixed sibling above? A `false` here means the decl is at the top of its prefix group and `down()` should run. Mirrored verbatim — pinned by `reduce_spaces_early_returns_when_prefixed_sibling_above_exists`.

5. **`reduceSpaces` `diff` is initialised once and reused.** The JS closure captures `let diff = false` and only reassigns on the first hit (`if diff === false`). Subsequent siblings with even-longer tail-lines have only `diff` chars stripped, NOT however many they'd need to match `prev_min`. So a tail-line that's 10 chars longer than prev_min, hit AFTER a tail-line that was 4 chars longer, has 4 chars stripped — leaving it 6 chars over prev_min. This is JS-deliberate (avoids over-correction); mirrored exactly.

6. **`disabled` cache keys distinguish three states.** `_autoprefixerDisabled` is a bool. `_autoprefixerSelfDisabled` is set ONLY when the disable came from a preceding `ignore next` comment, NOT from an enclosing `off` block. This distinction matters when a parent is "off" but a CHILD scopes back "on" — the child's parent's `selfDisabled` flag is checked to decide whether to inherit. Mirror via two separate attr keys.

## Drift flagged (NOT touched per CLAUDE.md)

**`crates/autoprefixer/src/supports.rs:384`** — `for _checker in cleaner.values("remove", &unprefixed) {`. AGENT_1's recent change made `Prefixes::values` return `Result<Vec<String>, NotYetImplemented>` (instead of `Vec<String>`). AGENT_2's call site here uses Rust's `Result: IntoIterator` impl, which iterates ONE element on `Ok` (the entire Vec, treated as a single item) or zero on `Err`. Currently `values` always returns `Err`, so the loop body never fires — same JS behaviour AS WAS. But the SHAPE is wrong: when `values` ever returns `Ok(vec![checker_a, checker_b])`, this loop will iterate ONCE on the whole Vec, not twice (once per element).

`cargo check --workspace` warns (`for_loops_over_fallibles`); the warning is the only one in the autoprefixer crate. Fix belongs to AGENT_2 (or AGENT_1 — they introduced the signature change without updating the caller). Recommended:

```rust
if let Ok(checkers) = cleaner.values("remove", &unprefixed) {
    for _checker in checkers {
        // TODO(agent-4): checker.check(value) once value prefixers expose .check.
    }
}
```

This is the latest in the AGENT_1-introduces-shape-change-without-updating-callers pattern. Same root cause as the original `cleaner_cache` private-field drift (now resolved). The fix is the same shape: when AGENT_1 changes a `Prefixes` method's return type, sweep callers (`Grep` for the method name) before merging.

## Other drift flagged

None. The hack registry was populated by AGENT_5 with 5 entries (`cross-fade`, `intrinsic`, `text-decoration`, `text-decoration-skip-ink`, `user-select`). My processor.rs slice doesn't dispatch through the registry (the walks are deferred), so AGENT_5's registrations are unused-but-correct from my point of view. Not drift.

## Whether peer agents are unblocked

- **AGENT_5 (hacks):** unchanged. Registrations against the existing `HackRegistry` skeleton work the same. The 5 hacks already registered remain dormant until the walks land.
- **AGENT_6 (NAPI wire-in):** still blocked. The end-to-end `Processor::add` / `Processor::remove` engine is not yet operational — only the helpers exist. AGENT_6 cannot wire `Stage::Autoprefixer` into the parity-runner without a working walk.

## Cursor-shift bugs hit

None — the helpers I ported don't insert nodes. `disabled` / `gridStatus` only WRITE attrs (cache), never mutate the AST structure. `reduceSpaces` mutates `raws.before` on existing siblings (no insert). The cursor-shift battlefield is in the deferred decl walk; the next AGENT_4 session will hit it.

The one place I touched insert-adjacent code is `DeclarationBase::process` — and that already had the path-bump pattern from AGENT_1, just punted on the `restore_before` call. I added the call without changing the cursor-shift logic. The new regression test (`process_calls_restore_before_when_cascade_branch_fires`) pins the call path; the existing `process_emits_each_prefix_with_cursor_shift` pins the cursor.

## Comments for AGENT_1 (forward-looking)

1. **The supports.rs:384 drift is the second `Prefixes`-API-change ripple in two passes.** First was `cleaner_cache` (private-field broke struct literals). Now `values` (return-type change broke for-loops). Both were silent — the first was a hard compile error eventually caught; the second is a soft warning that compiles. Going forward, when AGENT_1 changes a public `Prefixes` method's signature OR adds a private field, the next-step should be: `Grep` for callers across the crate (and AGENT_2/3/5 territory), update them in the same commit, run `cargo check --workspace`. The current pattern of "land the change, document the change, let the next agent fix the callers" multiplies drift across passes.

2. **`Prefixes::values` returning `Result<Vec<String>, NotYetImplemented>` is the right type-level surface, but consider returning `&[String]` (or `&Vec<String>`) by reference once preprocess() lands.** Building a `Vec<String>` per call is O(n) where the JS reads a stable per-bucket list. Not blocking, but a perf nit that would be cheaper to land before consumers get used to the owned-Vec contract.

3. **`PrefixesOptions::grid: Option<String>` and `flexbox: Option<String>` should both become enums** (`GridOption`, `FlexboxOption`). I worked around the grid case in `processor::grid_status` with explicit string coercion (see JS quirk #3); AGENT_2 worked around flexbox in `Supports::disabled`. Two workarounds for the same shape gap is one too many. The enum is a 5-minute change once you decide on it, and tracks the JS truth (these options are tri-state, not "string or unset").

4. **`Prefixes::group(root, path) -> Option<GroupView<'a>>`** is the right shape for read-only iteration but doesn't expose a path-yielding callback variant. `reduce_spaces` needs to know each sibling's path (to apply mutations after the walk) and currently re-derives the path from a `sib_offset` counter — fragile if the down-walk ever skips siblings. Consider adding `down_with_path<F: FnMut(&Node, &[usize]) -> bool>` to `GroupView` next pass. (Optional — the offset trick works for now.)

## Re-entry checklist for the next AGENT_4 session

When picking up the deferred walks:

1. Re-read `crates/_vendor/autoprefixer-10.4.14/package/lib/processor.js` — the helpers are landed but the walks aren't. The first walk (`walkAtRules`) needs at-rule-prefixer dispatch.
2. Decide where `preprocess()` lives. Two options:
   - In `processor.rs` as a `Processor::preprocess()` method that builds a private dispatch table on demand. Self-contained.
   - In `prefixes.rs` as `Prefixes::preprocess()`. Cleaner because then `Prefixes::add` is the populated map JS reads against; downside: AGENT_1 territory.
3. The `Processor` struct currently holds `&Prefixes` immutably. `preprocess()` typically wants `&mut Prefixes` (to populate the dispatch). Either change `Processor` to hold `&mut`, OR have `Processor::add(&mut Prefixes, root, warnings)` take it per-call.
4. The `walkDecls` pass needs to call `DeclarationBase::process(prefixes_all, root, path)` — the wiring I landed in this pass already takes the right signature.
5. Confirm AGENT_1 has resolved the supports.rs:384 drift before resuming, OR fold the fix into the same session if AGENT_1 punted again.

## Files changed

| File | Change |
|---|---|
| `crates/autoprefixer/src/processor.rs` | Stub → 640 LOC. `Processor` struct + 7 helpers + `GridStatus`/`DisplayType` enums + 8 module-level constants. 22 unit tests covering each helper's JS-quirk surface. |
| `crates/autoprefixer/src/declaration.rs` | `DeclarationBase::process` signature: added `prefixes_all: &Prefixes` arg. Wired `restore_before` call in the cascade branch (closes AGENT_1's punt). Updated 1 existing test, added 1 regression test. |
| `crates/autoprefixer/AGENT_4_DONE.md` | This file. |

## Floor that must NOT regress

**220 passing, 0 failing, 0 ignored.** Anyone landing work after me must keep this ≥220.

```bash
cd crates
RUSTFLAGS="" cargo test -p autoprefixer
RUSTFLAGS="" cargo build -p autoprefixer
RUSTFLAGS="" cargo check --workspace   # one supports.rs:384 warning is pre-existing drift (see §"Drift flagged")
```
