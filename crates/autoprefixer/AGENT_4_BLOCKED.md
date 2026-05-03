# AGENT_4 — BLOCKED on AGENT_1

Wrote nothing. No code changes. No test count delta. Floor untouched
(53 unit + 4 data parity + 3 browserslist parity = 60 passing, as
HANDOVER.md §1 reports).

## Why blocked

AGENT_4.md's "Hard pre-conditions" §1 requires AGENT_1 to have landed
`Prefixes::new` + `cleaner` + `select` + `group` before `processor.rs`
work begins, with `crates/autoprefixer/AGENT_1_DONE.md` as the gate
signal.

Verified at session start (timestamp: see git log of this commit if
landed):

- `crates/autoprefixer/AGENT_1_DONE.md` — **does not exist**.
- `crates/autoprefixer/src/prefixes.rs:136` — `Prefixes::new` is
  `unimplemented!("Phase 7 — port prefixes.js constructor; depends on
  data/prefixes.rs")`.
- `crates/autoprefixer/src/prefixes.rs:145` — `cleaner` is
  `unimplemented!("Phase 7 — port prefixes.js::cleaner")`.
- `crates/autoprefixer/src/prefixes.rs:150` — `select` is
  `unimplemented!("Phase 7 — port prefixes.js::select")`.
- `crates/autoprefixer/src/prefixes.rs:157` — `group` is
  `unimplemented!("Phase 7 — port prefixes.js::group")`.
- HANDOVER.md §1 "What's stubbed" line 34–36 confirms the orchestrator
  methods are not yet landed.

## Why I cannot work around it (the deeper block)

Beyond the four named methods being `unimplemented!()`, the `Prefixes`
struct's **surface shape** that `processor.js` consumes is not yet
laid down in Rust. Reading
`crates/_vendor/autoprefixer-10.4.14/package/lib/processor.js` end to
end, the engine touches:

- `this.prefixes.add[<key>]` — an indexed map of `Prefixer`-derived
  instances per name (NOT the `IndexMap<String, Vec<String>>` flat
  table currently on the Rust struct), each with a `.process(decl,
  result)` and `.prefixes` field. Used at lines 48–51, 82–84, 105–106,
  340, 345, 353, 359, 369–371.
- `this.prefixes.add.selectors` — a `Vec<Selector>`-shaped slot on
  the same map (line 82).
- `this.prefixes.remove[<key>]` and `this.prefixes.remove.selectors`
  — the symmetric structure for the remove pass (lines 404, 407, 421,
  444–446).
- `this.prefixes.transition` — a `Transition` instance (lines 334,
  439). AGENT_3's territory; once landed, AGENT_1 must wire it onto
  the `Prefixes` struct.
- `this.prefixes.options` — the `Options` value (`.supports`,
  `.flexbox`, `.grid`). Lines 64, 338, 503, 524, 700–701.
- `this.prefixes.unprefixed(prop)` — line 388, 435.
- `this.prefixes.values(kind, name)` — line 389, 470.
- `this.prefixes.normalize(prop)` — line 448.
- `this.prefixes.group(decl)` — line 447, 475, 597, 609. (Method
  exists in Rust, body unimplemented.)

None of these (`add`, `remove`, `options`, `transition`, `unprefixed`,
`values`, `normalize`) are present on `crates/autoprefixer/src/prefixes.rs`'s
`Prefixes` struct today. The current Rust struct has only
`browsers` + `add_table` + `remove_table` (the latter two as
`IndexMap<String, Vec<String>>`, which is the data-table view, not
the populated-prefixer view that `processor.rs` needs to call
`.process(decl)` against).

Per AGENT_4.md "What you must NOT do" §8: *"Do NOT add new methods to
base traits. If you need something missing, FILE A NOTE in your
handover and pause. Other agents wrote those shapes against your
eventual call sites — changing them silently breaks them."* The same
spirit applies to the `Prefixes` struct: shaping it is AGENT_1's
job. If I were to invent the field set, AGENT_1 would have to either
adopt my shape (forcing them into a constraint I had no authority to
set) or refactor my work (drift, churn).

Per CLAUDE.md "DRIFT DETECTION": *"DONT try and 'WORK AROUND' drift;
That's not your call to make. Drift is the enemy."*

## What AGENT_1 needs to land before AGENT_4 can resume

The shape I am waiting on (derived from `processor.js` 1:1):

1. `Prefixes::new(data, browsers, options)` — populated, with `add`
   and `remove` fields shaped as `IndexMap<String,
   PrefixerVariant>` (or analogous enum) where the variant carries
   the right base-class instance per name. Plus the `selectors` slot
   on each, plus a `transition: Transition` field, plus `options:
   Options`.
2. `Prefixes::cleaner(&self) -> &Prefixes` — landed body.
3. `Prefixes::select(&mut self, list)` — landed body.
4. `Prefixes::group(&self, decl) -> GroupView` — landed body. Returns
   a struct with `.up(callback)` and `.down(callback)` methods (see
   HANDOVER.md §4 for the locked semantics).
5. `Prefixes::unprefixed(&self, prop) -> &str`, `Prefixes::values(&self,
   kind, name) -> &[ValueChecker]`, `Prefixes::normalize(&self,
   prop) -> &str` — accessor methods derived from the JS originals
   in `prefixes.js`.
6. `crates/autoprefixer/AGENT_1_DONE.md` written, with the test count
   delta and any quirks AGENT_1 found.

Once those are green, I can write `processor.rs` against the locked
shape and unit-test it without `unimplemented!()` panics.

## What I will NOT do while waiting

- Will not stub a temporary `Prefixes` shape just to start writing
  `processor.rs`. That's the exact path to drift.
- Will not implement small orthogonal helpers from `processor.js`
  (`disabled`, `disabledDecl`, `disabledValue`, `gridStatus`,
  `displayType`, `withHackValue`, `reduceSpaces`). They DO call into
  `this.prefixes.options.{grid,flexbox}` and `this.prefixes.group`
  (line 597, 609 — `reduceSpaces`), so they're not actually
  shape-independent. Even the ones that don't would be untestable
  end-to-end without the surrounding walk.
- Will not commit anything. This file is the only artifact.

## Re-entry checklist for the next AGENT_4 session

When AGENT_1_DONE.md lands, the next AGENT_4 session should:

1. Re-read this file + AGENT_1_DONE.md + HANDOVER.md §3 (cursor-shift)
   + §4 (`group` semantics) + §7 (`_autoprefixer*` keys).
2. Re-verify `cargo test -p autoprefixer` floor before starting.
3. Confirm `Prefixes` struct surface matches the JS-side fields
   listed above; if AGENT_1 deviated, note it before writing against
   the deviation.
4. Then proceed per AGENT_4.md's "Scope discipline" — slice 0→100%
   if the unit is too big in one session.

— AGENT_4 (blocked, no work landed)
