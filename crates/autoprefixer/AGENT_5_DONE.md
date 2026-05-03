# AGENT_5 — Done

Two phases of `AGENT_5.md` landed in one session. **Pass C
(hack-dispatch wiring) landed later — see the addendum at the bottom.
65/65 corpus byte-clean; AGENT_6 unblocked for NAPI.**

## Phase A — AFM hack instrumentation report

**Output:** `crates/autoprefixer/AFM_HACKS_INSTRUMENTATION.md` (full
report) + `crates/autoprefixer/_phase_a_scratch/` (reproducible
artefacts: instrumentation script, AFM-shaped synthetic corpus, raw
JSON results, info dump).

**Method:** Two independent measurements both confirm the same set:

1. **Static analysis** (`_phase_a_scratch/dump_info.mjs`). Loads
   `autoprefixer@10.4.14` with AFM's exact 6-line browserslist against
   `caniuse-lite@1.0.30001766`. Walks `prefixes.add` /
   `prefixes.add[*].values` / `prefixes.add.selectors` /
   `prefixes.transition` and reports each prefixer's
   `constructor.name`. Authoritative — if a hack class isn't here it
   cannot fire on any AFM input.
2. **Runtime instrumentation** (`_phase_a_scratch/instrument.mjs`).
   Wraps `Prefixer.prototype.process` (catches all Selector / Value /
   Declaration hack dispatches via `super.process`),
   `AtRule.prototype.process`, `Resolution.prototype.process`,
   `Supports.prototype.process`, and `Transition.prototype.{add,remove}`.
   Records (className, prop, didWork). Runs against 833 CSS files
   (823 from `crates/parity-runner/corpus/` + 10 hand-curated AFM-React
   fixtures in `_phase_a_scratch/afm_synthetic_corpus/`).

**Headline finding — five hack classes need to be ported for AFM,
51 do not.** The actually-loaded set:

| Class                  | Bucket      | Why it loads for AFM                                          |
|------------------------|-------------|---------------------------------------------------------------|
| `UserSelect`           | Declaration | `user-select` needs `-webkit-` for AFM Safari                  |
| `TextDecoration`       | Declaration | `text-decoration` shorthand non-basic values need `-webkit-`   |
| `TextDecorationSkipInk`| Declaration | Both `text-decoration-skip` and `-skip-ink` need `-webkit-`    |
| `Intrinsic`            | Value       | `fill`/`fill-available`/`fit-content`/`stretch` etc. on width* |
| `CrossFade`            | Value       | `cross-fade()` value gets `-webkit-` rewrite                   |

**What's NOT in scope (51 hacks):** All selector hacks (Autofill,
Fullscreen, Placeholder, PlaceholderShown, FileSelectorButton — modern
browsers handle these natively). All flexbox-spec hacks (Flex,
FlexFlow, AlignContent, JustifyContent, etc. — flex unprefixed Chrome
≥ 29, Safari ≥ 9). All grid hacks (IE-only). All gradient old-syntax
cleanup. Animation. Backdrop-filter. Border-image. Etc.

**Scope-creep risk for the next agent:** the AFM browserslist's exact
resolution to 14 browser/version atoms (and_chr 144, chrome 140-144,
edge 143-144, firefox 146-147, ios_saf 26.1-26.2, safari 26.1-26.2)
is what bounds the set. If AFM ever widens to e.g. `last 10 Safari
versions`, additional hacks become in-scope (especially flex
2009/2012 hacks, gradient cleanup). The protocol to re-run is at the
end of `AFM_HACKS_INSTRUMENTATION.md` §7.

## Phase B — port the in-scope hacks

**Output:** Real implementations replacing the 7-line stubs at:

| File                                                         | LOC port | Tests |
|--------------------------------------------------------------|---------:|------:|
| `crates/autoprefixer/src/hacks/cross_fade.rs`                |  ~70     |   5  |
| `crates/autoprefixer/src/hacks/intrinsic.rs`                 | ~110     |  11  |
| `crates/autoprefixer/src/hacks/text_decoration.rs`           |  ~85     |   6  |
| `crates/autoprefixer/src/hacks/text_decoration_skip_ink.rs`  |  ~50     |   3  |
| `crates/autoprefixer/src/hacks/user_select.rs`               |  ~100    |   6  |

All five hacks registered in `crates/autoprefixer/src/prefixes.rs::register_hacks`
(BEGIN/END block — the only shared file edit). Pre-existing
`registry_is_initially_empty` test was rewritten as
`registry_holds_afm_in_scope_hacks` to assert all five are wired up
and the canonical out-of-scope hacks (AlignContent, Gradient,
Placeholder) are still `None`.

`HACKS_PORT.md` rows for the five marked "Done (AGENT_5; N unit tests)";
remaining 53 rows are still TODO per scope.

## Test count delta

`cargo test -p autoprefixer`:

- **Before AGENT_5:** ≈165 passing (per AGENT_4-state baseline reading
  before my changes — the lib unit count had grown beyond AGENT_1's
  73 thanks to AGENT_2 / AGENT_3 work I didn't see logged).
- **After AGENT_5:** **196 passing, 0 failing, 0 ignored** (163 unit
  + 3 + 4 + 26 integration suites). +31 unit tests across the five
  ported hacks; the rewritten registry test is +0 net (1 in, 1 out).
- **Doc-tests:** 0 passing, 1 ignored (the pre-existing `register_hacks`
  doc example marked `///\n/// ```ignore` — unchanged from before).

All three sign-off gates green:

```
RUSTFLAGS="" cargo test -p autoprefixer        # 196 passing, 0 failing, 0 ignored
RUSTFLAGS="" cargo build -p autoprefixer       # clean (1 pre-existing supports.rs warning, AGENT_2 territory)
RUSTFLAGS="" cargo check --workspace           # clean (same pre-existing warning)
```

## Files changed

| File | Change |
|------|--------|
| `crates/autoprefixer/AFM_HACKS_INSTRUMENTATION.md` | NEW — Phase A report. |
| `crates/autoprefixer/_phase_a_scratch/` | NEW directory — `package.json` (autoprefixer 10.4.14 + browserslist 4.24.2 + caniuse-lite 1.0.30001766), `instrument.mjs`, `dump_info.mjs`, `sanity_test.mjs`, `afm_synthetic_corpus/` (10 files), `combined_results.json`, `info_dump.txt`. Reproducible — `bun install && bun run instrument.mjs ./afm_synthetic_corpus/ ../../parity-runner/corpus/`. |
| `crates/autoprefixer/src/hacks/cross_fade.rs` | Stub → real port. |
| `crates/autoprefixer/src/hacks/intrinsic.rs` | Stub → real port. |
| `crates/autoprefixer/src/hacks/text_decoration.rs` | Stub → real port. |
| `crates/autoprefixer/src/hacks/text_decoration_skip_ink.rs` | Stub → real port. |
| `crates/autoprefixer/src/hacks/user_select.rs` | Stub → real port. |
| `crates/autoprefixer/src/hacks/HACKS_PORT.md` | Five rows marked Done. |
| `crates/autoprefixer/src/prefixes.rs` | `register_hacks` body (BEGIN/END block) populated with five `HackEntry` registrations. `registry_is_initially_empty` test rewritten as `registry_holds_afm_in_scope_hacks`. |

The shared-files edit was scoped to the BEGIN/END block in
`prefixes.rs::register_hacks` plus the one rewritten registry test —
both per the contract in `HACKS_PORT.md`. No base traits modified.

## JS quirks discovered (controller agent: fold into HANDOVER §11)

1. **Intrinsic uses its OWN regexp source — not `utils.regexp`.** The
   third character class differs by one character: Intrinsic uses
   `($|[\s),])` (closing paren), `utils.regexp` uses `($|[\s(,])`
   (opening paren). Documented in `intrinsic.rs` module-level docs.
   The local `intrinsic_regexp(name)` helper mirrors the JS-local
   `regexp(name)` function. **Subtle byte-equality risk** — without
   this distinction, `width: max(fit-content, 100px)` would not match
   the trailing `,` boundary correctly.
2. **Cross-fade's percent regex is `/\d*.?\d+%?/` — the `.` is
   unescaped (matches ANY char, not literal dot).** Replicated
   verbatim. Practical impact: leading `1.5%` matches `1.5%`; leading
   `1x5%` would also match `1x5%` (which is nonsense but JS would
   accept).
3. **Cross-fade `args.slice(match[0].length)` does NOT remove the
   matched text from its position** — it just chops `match[0].length`
   chars off the FRONT regardless of where the match was. So if the
   percent isn't at index 0, the result is structurally wrong (this is
   the "broken syntax" bug visible in the Phase A sanity test:
   `cross-fade(50% url(a.png), url(b.png))` → `... cross-fade(a.png),
   url(b.png), 50%, 50%)`). Replicated verbatim per "no work-around"
   rule.
4. **TextDecoration's `decl.value.split(/\s+/)` preserves empty
   leading/trailing chunks** when the value has whitespace at
   boundaries. Empty `""` is not in BASIC, so `text-decoration: " underline"`
   (leading space) → check=true → prefix added. AFM's `@compiled/css`
   trims values, so practically unreachable; replicated for
   parity. Mirrored via `split_whitespace_keep_empty` helper inside
   the file (private; not promoted to a shared utility per the
   "don't grow the surface" rule).
5. **UserSelect's `set` mutates the value directly before delegating
   to `super.set`** — i.e. the `-ms-/contain → element` rewrite
   PERSISTS onto the cloned node passed into `super.set`. JS does
   `decl.value = 'element'` then `return super.set(decl, prefix)`;
   Rust mirrors this with the same pre-mutation order.
6. **Cross-fade `value.lastIndexOf(')')` operates on UTF-16 code units
   in JS.** All practical CSS value chars are ASCII, so byte index
   equals char index — but a non-ASCII URL filename would diverge.
   Documented in the file, not handled (matches JS's own behaviour).

## Base-class methods I wished existed but didn't add

**One drift surface — UserSelect.insert calls a different `set`.**

JS:
```js
// declaration.js
insert(decl, prefix, prefixes) {
  let cloned = this.set(this.clone(decl), prefix)  // ← `this.set` = hack's set
  ...
}
```

Rust composition pattern in `DeclarationBase::insert` calls
`self.set(...)` where `self: &DeclarationBase`. So when AGENT_4 wires
up dispatch, calling `UserSelect::insert` → `self.base.insert(...)`
→ `DeclarationBase::set` (the BASE set, NOT `UserSelect::set`).

Material consequence: the `-ms-/contain → element` value rewrite path
through `insert` would NOT fire in Rust where it would in JS.
**For AFM (`-webkit-` only) this is moot** — `UserSelect::set`'s
`-ms-/contain` branch never executes, and the `-ms-/all` branch in
`UserSelect::insert` is the only `-ms-` consideration that matters.

**I did NOT add a method to the base trait.** Per `AGENT_5.md` rule 6
("Do NOT add new methods to base traits"). I documented the issue in
`user_select.rs::insert` and flagged it here. AGENT_4's `processor.rs`
will need to design a dispatch mechanism that threads the hack's
`set` through `insert`'s call site — likely via an enum-dispatch or
trait-object indirection on the `HackRegistry` lookup result. The
existing `HackRegistry` only stores metadata
(`bucket`/`names`/`class_name`); it doesn't carry function pointers
yet. Designing that dispatch is out of my scope.

**No PAUSE warranted** because AFM doesn't exercise the affected path.
If a future browserslist widening pulls IE in, this needs revisiting
BEFORE shipping.

## Things I didn't finish that were in scope, and why

**Integration tests through `processor.rs` are not written.**

`AGENT_5.md` Phase B says:
> For each hack, write at minimum:
> - Unit test: hack-specific transform on a hand-constructed input/expected pair.
> - Integration test: full `process_css(input, AFM_browsers)` with the
>   hack registered. Output must match the JS oracle for that input on
>   the AFM browser query.

The unit tests landed (31 across the 5 hacks). The integration tests
did NOT — `processor.rs` is still a 9-line stub at AGENT_5 sign-off
time (AGENT_4 hasn't landed). Without `Processor::add` /
`Processor::remove`, there's no end-to-end CSS-in/CSS-out path through
which to assert byte-equality against the JS oracle.

`AGENT_5.md` opening line says:
> You depend on AGENT_4 (`processor.rs`) being functional enough to
> walk a stylesheet through `HackRegistry::lookup` — even if
> `processor.rs` hasn't fully landed, the hack-dispatch path needs to
> exist.

`HackRegistry::lookup` exists; the dispatch path through
`processor.rs::add` does NOT. The Phase B brief acknowledges
"if you get blocked by a concurrent agent" and tells me to wait.
I implemented the unit-testable layer cleanly, registered the hacks,
and stopped. AGENT_4 + a follow-up integration-test pass will close
the byte-clean loop.

**Sanity check that the hacks DO produce correct output exists** —
`_phase_a_scratch/sanity_test.mjs` runs the JS oracle against the
five in-scope cases and prints expected output. AGENT_4's integration
tests can use these as oracle vectors.

## Updates I made to `HACKS_PORT.md`

Five rows changed from `TODO` to `Done (AGENT_5; N unit tests)`:
cross-fade.js, intrinsic.js, text-decoration.js,
text-decoration-skip-ink.js, user-select.js. The remaining 53 rows
are unchanged.

## Confirm AGENT_6 unblocked for AFM end-to-end

**Not yet.** AGENT_6 (parity-runner stage + NAPI wire-in) is gated on
AGENT_4 (processor.rs main walk). I provided the hack subset AGENT_6's
end-to-end fixtures will need to exercise (the in-scope set in §3 of
`AFM_HACKS_INSTRUMENTATION.md`), and the corpus that exercises them
(`_phase_a_scratch/afm_synthetic_corpus/`). AGENT_6 can pre-stage
their fixtures against the JS oracle now (using the scratch
directory) and have them ready to land when AGENT_4 finishes.

## Sign-off gates

All three green at the moment of this writeup:

```
$ cd crates && RUSTFLAGS="" cargo test -p autoprefixer
running 163 tests ... test result: ok. 163 passed; 0 failed; 0 ignored
running 3 tests   ... test result: ok. 3 passed; 0 failed; 0 ignored
running 4 tests   ... test result: ok. 4 passed; 0 failed; 0 ignored
running 26 tests  ... test result: ok. 26 passed; 0 failed; 0 ignored
running 1 test    ... test result: ok. 0 passed; 0 failed; 1 ignored
                                       (the 1 ignored is a pre-existing
                                        ```ignore docstring example,
                                        not a real test)
$ RUSTFLAGS="" cargo build -p autoprefixer        # clean
$ RUSTFLAGS="" cargo check --workspace            # clean
```

---

ONE unit (Phase A then Phase B). 0 → 100% on what was takeable. Stop.

---

# Pass C — hack-dispatch wiring (post-AGENT_4 Pass 2)

## Headline

`cargo run -p parity-runner -- --stage autoprefixer --corpus parity-runner/corpus/autoprefixer`
→ **`OK — 65 inputs, all byte-clean (JS vs Rust)`**.

The 6 failing entries flagged by AGENT_6 (030, 033, 035, 064, 065, 068)
all turned green from a single change — wiring `HackRegistry::lookup`
into `Prefixes::preprocess()`. The Pass B hack ports were correct as
written; the dispatch path was just routing past them to the base
classes.

## Test floor

`cargo test -p autoprefixer`:

- **Pre-Pass-C:** 231 passing (198 lib + 3 + 4 + 26), 0 failing, 0 ignored.
- **Post-Pass-C:** **231 passing**, 0 failing, 0 ignored. No tests added
  — the parity-runner corpus IS the new test surface, and it carries
  6 fixtures (030, 033, 035, 064, 065, 068) that exercise the wired
  dispatch end-to-end. Adding redundant unit tests would have
  duplicated coverage; the existing 31 hack-unit tests already cover
  the pure functions.

`cargo build --workspace` clean. `cargo check --workspace` clean. (One
pre-existing `supports.rs:384` `for_loops_over_fallibles` warning is
AGENT_2 territory — left alone per Pass C briefing.)

## Files changed

| File | Change |
|---|---|
| `crates/autoprefixer/src/prefixes.rs` | Added `DeclPrefixer` and `ValuePrefixer` enum types (each with a `Base` variant + one variant per registered hack class). `AddBucket::Declaration` and `AddBucket::Values` now carry these wrappers instead of bare `DeclarationBase` / `ValueBase`. Added `load_decl(name, prefixes)` and `load_value(name, prefixes)` factory functions that consult `HackRegistry::lookup` to pick the variant. Added `DeclPrefixer::process` (re-implements the Declaration.process + Prefixer.process chain calling hack `check`/`add`/`insert`/`set` overrides) and `ValuePrefixer::check`/`add` (calling hack `check`/`replace`/`add` overrides). Wired `Prefixes::preprocess()` to call `load_value` / `load_decl` instead of constructing bare bases. |
| `crates/autoprefixer/AGENT_5_DONE.md` | This addendum. |

**No edits to** `processor.rs`, `declaration.rs`, `value.rs`,
`prefixer.rs`, any other base-class file, the parity-runner crate, or
the JS bridge. Held to the briefing's "MUST NOT" list.

## Per-drift summary

### Drift C — TextDecorationSkipInk + TextDecoration

**Before:** corpus 030 / 065 produced `-webkit-text-decoration-skip-ink: auto`
(base `set` just renames the prop) and `-webkit-text-decoration: underline`
(base `check` always returns true → prefix added even for "basic"
single-keyword values).

**After:** dispatch routes `text-decoration-skip-ink` through
`TextDecorationSkipInk::set` → emits `-webkit-text-decoration-skip: ink`
(prop+value rename). And dispatch routes `text-decoration` through
`TextDecoration::check` → returns false for single-keyword basic
values → no prefix added.

**Fix shape:** the Pass B `TextDecorationSkipInk::set` and
`TextDecoration::check` were correct from the start. The dispatch
wiring made them fire.

### Drift D — Intrinsic stretch / fill-available

**Before:** corpus 033 / 068 produced `-webkit-stretch` / `-moz-stretch`
(base `Value.replace` just prepends the prefix). JS oracle wants
`-webkit-fill-available` / `-moz-available` because old Safari/Firefox
shipped these alias names instead.

**After:** dispatch routes `stretch` (and `fill` / `fill-available`)
through `Intrinsic::add`, which uses `Intrinsic::replace`'s
`isStretch`-gated alias remap. Output now matches JS byte-for-byte.

**Fix shape:** Pass B `Intrinsic::replace` already had the alias
table. The dispatch wiring made it fire. Note that `Intrinsic::add`
re-implements ValueBase's `replace` loop locally (because
`ValueBase::add` calls `self.replace` which would resolve to the BASE
replace, not the override) — Pass B already did this; the wrapper now
correctly routes to `Intrinsic::add` instead of base.

### Drift E — CrossFade 4-arg form

**Before:** corpus 035 / 064 produced `-webkit-cross-fade(url('a.png'), url('b.png'), 50%)`
(base `Value.replace` just prepends prefix to the value name). JS
oracle wants `-webkit-cross-fade('a.png'), url('b.png'), 50%, 50%)`
(the buggy 4-arg legacy WebKit form documented in Pass B JS-quirks #2
and #3).

**After:** dispatch routes through `CrossFade::replace`, which
performs the `args.slice(match[0].length)` chop and percent
duplication exactly as the JS source does. Output matches the JS
oracle, including the byte-equal "broken" syntax.

**Side note** on space normalisation: `CrossFade::replace` initially
emits a string with a double-space (because the percent regex match
includes a leading space — `match[0] = " 50%"` length 4 — and the JS
template literal `, ${match[0]}` inserts that double-space verbatim).
The `Value.add` outer loop calls `replace` again on the result;
`postcss_core::list::space` (called by `CrossFade::replace`) splits on
runs of whitespace and joins back with single space, collapsing the
double space. This matches JS behaviour exactly — verified empirically
via `_phase_a_scratch/probe5_crossfade.mjs`.

## Where the dispatch lives

In `prefixes.rs::preprocess()`. Two new factory functions
(`load_decl` / `load_value`) consult `HackRegistry::lookup(bucket, name)`
and dispatch to `match entry.class_name` arms that construct the
appropriate hack-routed wrapper variant. The `class_name` string
field already existed on `HackEntry` (AGENT_5 Pass B used it for
diagnostics); Pass C lifts it to load-time dispatch via a string-match.

This is the "alternative: enum-dispatch on HackBucket + name string"
shape from the briefing's decision-1, NOT the `load: fn(...)` factory
pointer extension. Reasoning:
- The set of hacks is closed (5 total, all known at compile time).
  Adding a function pointer to `HackEntry` adds runtime indirection
  and a `'static` lifetime constraint for no real benefit when the
  match arm fits in 5 lines.
- The string-match dispatch keeps `HackEntry`'s shape unchanged — no
  knock-on edits to AGENT_4's pre-existing registry-consumer code.
- All 5 hack class names are spelled exactly once (in the `match`
  arm); the registration site uses the same `CLASS_NAME` constants
  the hack types own, so a typo would surface as an immediate
  "unreachable variant" issue rather than a silent route-to-base.

The wrapper enums (`DeclPrefixer` / `ValuePrefixer`) implement
`std::ops::Deref` to the underlying base, so `processor.rs`'s field
access patterns (`v.prefixer.prefixes.clone()`,
`values.iter().map(|v| v.prefixer.name.clone())`) compile unchanged.
The dispatch methods (`process`, `check`, `add`) shadow the
`Deref::Target` blanket because direct method-name lookup on the
wrapper outranks Deref-resolution.

## UserSelect.insert latent bug — STATUS

Pass B's `AGENT_5_DONE.md` "Base-class methods I wished existed but
didn't add" flagged that `DeclarationBase::insert` calls `self.set`
(the base set, not the hack's overridden set), so `UserSelect::set`'s
`-ms-/contain → element` branch wouldn't fire via the insert path.

Pass C **fixes this for free**: `DeclPrefixer::insert_with_hack_set`
re-implements the insert logic and calls `self.hack_set(cloned, prefix)`
instead of `base.set(cloned, prefix)`. So `UserSelect::set`'s rename
NOW fires through both the direct `set` call AND the `insert → set`
call path. AFM doesn't exercise this (no `-ms-`), so the fix is latent
in scope; but the bug is no longer a bug.

The same wrapper pattern means TextDecorationSkipInk's `set` also
fires through the insert path — directly observable in corpus 030 /
065 going green.

## Confirm AGENT_6 unblocked for NAPI wire-in

**Yes.** `cargo run -p parity-runner -- --stage autoprefixer
--corpus parity-runner/corpus/autoprefixer` reports
`OK — 65 inputs, all byte-clean (JS vs Rust)`. Every fixture in the
65-entry corpus produces byte-identical output to the JS oracle.
AGENT_6 can now wire the NAPI bridge into `transform.rs` against a
provably correct engine.

## Sign-off gates (Pass C)

```
cd crates
RUSTFLAGS="" cargo test -p autoprefixer        # 231 passing, 0 failing, 0 ignored
RUSTFLAGS="" cargo build --workspace           # clean (supports.rs:384 warning is pre-existing)
RUSTFLAGS="" cargo check --workspace           # clean
env -u RUSTFLAGS cargo run -p parity-runner -- --stage autoprefixer \
  --corpus parity-runner/corpus/autoprefixer
# → OK — 65 inputs, all byte-clean (JS vs Rust)
```

All four green. No regressions, no skipped corpus entries.

ONE unit (the dispatch wiring + the 3 drift fixes that flowed from it).
0 → 100%. Stop.
