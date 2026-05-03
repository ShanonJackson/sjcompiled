# AGENT_5 — AFM hack instrumentation + hack subset port

You are picking up two units inside the larger `crates/autoprefixer` port.
You have NO memory of the prior conversation — this file plus the docs
it points at are your full briefing.

You depend on AGENT_4 (`processor.rs`) being functional enough to walk
a stylesheet through `HackRegistry::lookup` — even if `processor.rs`
hasn't fully landed, the hack-dispatch path needs to exist.

---

## What you own — TWO sequential phases

### Phase A: AFM hack instrumentation report

**Output:** `crates/autoprefixer/AFM_HACKS_INSTRUMENTATION.md` —
empirical report of which autoprefixer hacks AFM's actual CSS reaches
during a real build. Same pattern that closed the browserslist gate
via `BROWSER_LIST_FROM_AFM.md`.

**Method:** Either
1. Instrument `crates/_vendor/autoprefixer-10.4.14/package/lib/processor.js`
   (the JS source — copy it to a scratch dir if you want, don't edit
   the vendored copy) to log every `HACK.process(node)` invocation.
   Run it against a representative AFM-style CSS corpus. Aggregate
   hack-name frequency.
2. OR: ask the user (Shanon) to coordinate with AFM's dependency
   engineer to run the same instrumentation in their actual
   `jira/` build pipeline (mirror the protocol used to capture
   `BROWSER_LIST_FROM_AFM.md`). This is more accurate.

Document in the report:
- The hack list AFM reaches, sorted by frequency.
- The hack list AFM does NOT reach (= deferred indefinitely).
- The CSS corpus size and source (synthetic vs. AFM-actual).
- The AFM browser query that gated the hack decisions
  (`crates/browserslist-shim/tests/fixtures/afm/.browserslistrc`).

### Phase B: Port the hacks AFM reaches

**Output:** Real implementations in `crates/autoprefixer/src/hacks/*.rs`
for the subset identified in Phase A. Plus registration entries in
`crates/autoprefixer/src/prefixes.rs::register_hacks` (the existing
BEGIN/END marked block is the only place you may edit outside
`src/hacks/`).

ALL OTHER 58 hacks remain stubs. Don't speculate-port.

The subset is likely 5–15 hacks based on what AFM uses. Common
candidates (AFM is a React-heavy SaaS app):
- `display-flex.js` (and the flex-* family) — flexbox is everywhere
- `placeholder.js`, `placeholder-shown.js` — input styling
- `gradient.js` — if AFM uses CSS gradients (likely)
- `transitions` (already covered by AGENT_3's `transition.rs`, NOT a hack)
- `user-select.js`
- `appearance.js` — form controls

Phase A confirms the actual list.

---

## Read these BEFORE writing code (in this order)

1. `crates/PARITY_VERSIONS.md` — byte-equality contract. ~5 min.
2. `crates/PLUGIN_IMPLEMENTATION_GUIDE.md` — AST surface, helpers.
   ~10 min.
3. `crates/autoprefixer/HANDOVER.md` — read all of it. ~20 min. Especially:
   - §1 (current floor)
   - §3 (cursor-shift bug — many hacks insert in loops)
   - §7 (`_autoprefixer*` keys — namespace anything you cache)
   - §11 (JS quirks)
4. `crates/autoprefixer/GATE_CLOSED_FOR_AUTOPREFIXER_AGENT.md` ~5 min.
5. `crates/autoprefixer/src/hacks/HACKS_PORT.md` — the per-hack
   parent-class table + LOC. Critical reference. ~5 min.
6. `BROWSER_LIST_FROM_AFM.md` (workspace root) — the precedent for the
   AFM instrumentation pattern. ~5 min.
7. AGENT_4_DONE.md if it exists, to confirm `processor.rs` dispatches
   through `HackRegistry::lookup`. ~5 min.
8. **For each hack you'll port (Phase B):** the vendored JS source at
   `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/<file>.js`,
   end-to-end. Per hack: 5–30 min depending on size.

---

## Where things stand at start

`cargo test -p autoprefixer` → **whatever AGENT_1/2/3/4 left it at**.
Your work must keep the floor intact and ideally grow it (one test per
hack ported, plus integration tests for AFM-end-to-end).

What's REAL:
- All base classes, `Prefixes::new`, `Supports`, `Transition`, `processor.rs`.
- `Browsers`, `data/prefixes.rs`, `prefixes.rs::HackRegistry` registration
  framework.

What's STUBBED:
- 58 hack files (all 7-line stubs at `crates/autoprefixer/src/hacks/*.rs`).
- `Stage::Autoprefixer` parity-runner stage + NAPI wire-in — AGENT_6.

---

## Hack porting contract (Phase B)

Per HANDOVER §"Phase 7 split contract" (in `crates/STATUS.md`), each
hack:

1. Sub-class composition: a hack `MyHack` wraps a base
   (`DeclarationBase` / `ValueBase` / `SelectorBase` / `AtRuleBase`)
   and adds hack-specific overrides for `prefixed`, `replace`,
   `cleanFromUnprefixed`, `process`, `set`, etc. — whichever methods
   the JS source overrides.
2. Trait surface: do NOT add new methods to the base traits. The base
   class shape is locked. If your hack genuinely needs a missing
   method, FILE A NOTE in your handover and PAUSE. Do not invent.
3. File mapping: `lib/hacks/foo-bar.js` → `src/hacks/foo_bar.rs`.
4. Registration: append to
   `crates/autoprefixer/src/prefixes.rs::register_hacks` in
   alphabetical-by-JS-filename order. The BEGIN/END markers there are
   the only edit zone outside `src/hacks/`.
5. Update `crates/autoprefixer/src/hacks/HACKS_PORT.md` row to mark Done.
6. `Node.attrs` keys: namespace with `_autoprefixer<HackName>` and add
   to `prefixer::CLONE_STRIP_KEYS` if you cache anything cloneable.

---

## Test discipline

Per HANDOVER §6: every test that uses `Browsers::new(...)` MUST set
`BrowsersOptions::from` explicitly. AFM fixture path:

```rust
use std::path::PathBuf;
fn afm_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("browserslist-shim").join("tests").join("fixtures").join("afm")
}
```

For each hack, write at minimum:
- Unit test: hack-specific transform on a hand-constructed input/expected pair.
- Integration test: full `process_css(input, AFM_browsers)` with the
  hack registered. Output must match the JS oracle for that input on the
  AFM browser query.

DO NOT byte-test against `defaults` / `> 1%` / `chrome >= 50` etc. —
those drift through the oxc fallback.

Generate JS oracle vectors via bun, mirror the `data_parity.rs` pattern.
For an integration test, run the actual `autoprefixer@10.4.14` JS via
bun against the AFM-pinned `browserslist@4.24.2` and dump the output.

---

## Cursor-shift bug — read HANDOVER §3

Many hacks insert prefix variants in loops (`gradient.js`, `display-flex.js`,
the flex family). The cursor-shift bug bites if you're not careful. The
pattern is in `at_rule.rs::process`. Heuristic: any test that exercises
≥2 prefixes per node MUST verify all clones land. Single-prefix tests
are silent.

---

## What you must NOT do

1. Do NOT touch `prefixes.rs::Prefixes` body (AGENT_1's territory),
   `supports.rs` (AGENT_2), `transition.rs` (AGENT_3), or `processor.rs`
   (AGENT_4). The ONLY shared file you may edit is the
   `register_hacks()` BEGIN/END block in `prefixes.rs`.
2. Do NOT edit `crates/parity-runner/`, `packages/css/`, or
   `crates/css/src/transform.rs`. AGENT_6 owns those.
3. Do NOT bump any pinned version.
4. Do NOT "fix" upstream bugs. Replicate.
5. Do NOT speculate-port hacks. ONLY port what Phase A's report shows
   AFM uses. The whole point of the instrumentation is to bound scope.
6. Do NOT add new methods to base traits. File a note + pause if
   needed.
7. Do NOT roll your own value/selector parsers. Use the workspace ones.
8. Do NOT use `HashMap` on the hashing path. `IndexMap` only.
9. Do NOT remove `oxc_browserslist` or widen the AFM grammar.
10. Do NOT re-port `flex-spec.js` or `grid-utils.js` as classes — they're
    shared helpers in the JS source. Port as plain functions in
    `hacks/flex_spec.rs` / `hacks/grid_utils.rs` per the existing
    convention (per `STATUS.md` "Phase 7 split contract").

---

## Sign-off gates — run all three before claiming done

```bash
cd crates
RUSTFLAGS="" cargo test -p autoprefixer        # must show ≥(prior floor) passing, 0 failing, 0 ignored
RUSTFLAGS="" cargo build -p autoprefixer       # clean
RUSTFLAGS="" cargo check --workspace           # clean
```

If anything fails and you can't fix in 10 min, ROLL BACK.

---

## What to write when done

Write `crates/autoprefixer/AGENT_5_DONE.md` with:
- The Phase A instrumentation report summary (full report stays in
  `AFM_HACKS_INSTRUMENTATION.md`).
- The exact list of hacks ported in Phase B, with test counts per hack.
- Hacks in the AFM list you DIDN'T port and why (timeout, bigger than
  expected, etc.) — these become the next-agent's unit.
- JS quirks discovered.
- Any base-class methods you wished existed and PAUSED on.
- Updates you made to `HACKS_PORT.md` (mark rows Done).
- Confirm AGENT_6 (parity-runner stage + NAPI wire-in) is unblocked
  for AFM end-to-end.

Do NOT update HANDOVER.md / MORNING.md / STATUS.md yourself.

---

## If you're stuck

Vendored JS at `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/`.
HANDOVER.md exhaustive.
HACKS_PORT.md has the per-hack parent class.

ONE unit (Phase A then Phase B). 0 → 100% on what you take. Stop.
