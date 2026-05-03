# AGENT_5 — Done

Two phases of `AGENT_5.md` landed in one session.

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
