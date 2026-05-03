# AGENT_1 — `Prefixes::new` orchestrator + entry shell

You are picking up a focused unit inside the larger `crates/autoprefixer`
port. You have NO memory of the prior conversation — this file plus the
docs it points at are your full briefing.

---

## What you own

ONE unit, taken 0 → 100% byte-clean against the JS oracle:

1. **`Prefixes::new`** — constructor body (`crates/autoprefixer/src/prefixes.rs`).
   Currently `unimplemented!()`. Maps to `prefixes.js` constructor (~150 LOC
   of the 428-LOC vendored file).
2. **`Prefixes::cleaner`** — returns a Prefixes configured for removal of
   stale prefixes. Maps to `prefixes.js::cleaner`.
3. **`Prefixes::select`** — pick add/remove targets from the data table.
   Maps to `prefixes.js::select`.
4. **`Prefixes::group`** — return a "group" view of declarations adjacent
   to a node. Used by `declaration.js::restoreBefore` and `isAlready`.
   Maps to `prefixes.js::group`. **HANDOVER.md §4 has the full semantics
   explanation — read it.**
5. **`info.rs` + `autoprefixer.rs` entry shell** — small, low-stakes. The
   entry shell wires `Browsers::new(...) → Prefixes::new(...) → process()`
   into one callable. Source: `lib/info.js` (only the bare module, NOT
   the diagnostic `info()` function — that's not on the hashing path),
   `lib/autoprefixer.js`.

That's the whole unit. Do NOT touch anything else.

---

## Read these BEFORE writing code (in this order)

1. `crates/PARITY_VERSIONS.md` — the byte-equality contract. ~5 min.
2. `crates/PLUGIN_IMPLEMENTATION_GUIDE.md` — how plugins integrate with
   `postcss-core`, the AST surface, the helpers you must use (not roll
   your own). ~10 min.
3. `crates/autoprefixer/HANDOVER.md` — exhaustive autoprefixer handover.
   Pay close attention to:
   - §1 (current floor — do not regress it)
   - §3 (cursor-shift bug — IF you touch any insert-in-loop pattern)
   - §4 (`Prefixes::group` semantics — load-bearing for your unit)
   - §6 (browserslist gate CLOSED, includes the test-discipline rule
     about always setting `BrowsersOptions::from` explicitly)
   - §7 (`_autoprefixer*` attribute keys — namespacing)
   - §11 (subtle JS quirks)
   ~20 min.
4. `crates/autoprefixer/GATE_CLOSED_FOR_AUTOPREFIXER_AGENT.md` —
   what the previous agent landed and what you can rely on. ~5 min.
5. `crates/browserslist-shim/AFM_PORT_NOTES.md` — architecture of the
   hybrid AFM-fast-path / oxc-fallback resolver. You need this because
   `Prefixes::new` consumes `Browsers::new(...)` which consumes the
   shim. ~5 min.
6. `BROWSER_LIST_FROM_AFM.md` (workspace root) — AFM's actual
   browserslist output. ~3 min.
7. **The vendored JS source** —
   `crates/_vendor/autoprefixer-10.4.14/package/lib/prefixes.js` lines
   1–428. Read end-to-end before writing a line of Rust. ~15 min.

Total: ~63 min of reading. Don't skip it.

---

## Where things stand

`cargo test -p autoprefixer` → **60 passing, 0 ignored** (53 unit + 4
data parity + 3 browserslist parity). That number is the floor; your
work must keep it there or grow it. Run it before EVERY commit.

What's REAL (full bodies, real tests; you depend on these):
- `utils.rs`, `vendor.rs`, `brackets.rs`, `old_value.rs`, `old_selector.rs`
- `prefixer.rs` (parent walk + cache + clone-strip)
- `browsers.rs` (caniuse-db agents + browserslist-shim hybrid resolver)
- `at_rule.rs`, `value.rs`, `selector.rs`, `declaration.rs`, `resolution.rs`
  (all base classes)
- `data/prefixes.rs` (183-entry static table, byte-clean against JS oracle)
- `prefixes.rs::HackRegistry` skeleton (you do NOT register hacks — that's
  AGENT_5's job)

What's STUBBED (you do NOT touch these — other agents own them):
- `supports.rs`, `transition.rs` — AGENT_2 / AGENT_3
- `processor.rs` — AGENT_4 (consumes your `Prefixes::new`; depends on you
  finishing first)
- All 58 hack files — AGENT_5
- `Stage::Autoprefixer` parity-runner stage + NAPI wire-in — AGENT_6

---

## The data shape you need to build

`Prefixes::new(browsers: Browsers, data: &PREFIXES) -> Prefixes` walks
the 183-entry `PREFIXES` table from `crates/autoprefixer/src/data/prefixes.rs`,
intersects each entry's `browsers: Vec<String>` field with
`browsers.selected: Vec<String>`, and emits two `IndexMap`s:

- `add_table` — keyed on the property/value/selector name; value is the
  list of prefixes that need to be ADDED (`["-webkit-", "-moz-"]`, etc.).
- `remove_table` — keyed the same way; value is the list of stale
  prefixes that need to be REMOVED for the current browser set.

The exact transform shape is in `prefixes.js` constructor lines 13-90
(roughly). Read it.

`HackRegistry` in `prefixes.rs` already exists as a registration table.
Your `Prefixes::new` should consume `registry()` to know which props are
hack-driven (e.g., `display: flex` is owned by the `display-flex` hack
once AGENT_5 ports it). For now the registry is empty — that's fine,
your code path should handle "no hack registered" gracefully (default to
the base class `Declaration` / `Value` / `Selector` / `AtRule`).

---

## `Prefixes::group(decl)` — load-bearing semantics

`declaration.js::restoreBefore` calls `this.all.group(decl).up(...)`.
The JS `group(decl)` returns an iterator-like object; `up(callback)`
walks BACKWARDS through the decl's siblings, calling `callback` on each
prefixed-equivalent decl until the callback returns truthy (= match
found) or a non-prefixed-equivalent sibling is hit.

In Rust, with the path-based AST: build a view that walks backwards
from `decl`'s path using `sibling_relative(root, path, -1)`, `(-2)`, ...
and yields each adjacent decl whose normalized prop matches
`vendor::unprefixed(decl.prop)`.

`Selector::already` (in `crates/autoprefixer/src/selector.rs`) already
implements the same shape. Read it for the pattern. The only difference
is `Selector` walks rule siblings; `Prefixes::group` walks decl siblings
inside the same Rule.

`declaration.rs::restore_before` is currently a no-op pending this. Once
your `Prefixes::group` lands, fill it in — the cascade tests in
`__tests__/cascade.test.js` of the vendored autoprefixer source will
fail visibly if you forget.

---

## Test discipline — DO NOT rely on cwd

Per HANDOVER §6 closing paragraphs: every test that consumes
`Browsers::new(...)` MUST set `BrowsersOptions::from` explicitly. The
gate-closure agent's `parse_static` change defaults `path` to
`std::env::current_dir()` when `from` is unset. Cargo's test-binary cwd
varies between invocations — a non-AFM cwd silently lands on a different
`.browserslistrc` walk result (or none, falling through to the
oxc-fallback `defaults` which drifts). Your byte-test will then fail
for reasons unrelated to your code.

Use:

```rust
use std::path::PathBuf;
fn afm_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()                   // crates/
        .join("browserslist-shim")
        .join("tests").join("fixtures").join("afm")
}

let opts = BrowsersOptions {
    from: Some(afm_fixture_dir().to_string_lossy().into_owned()),
    ..Default::default()
};
let browsers = Browsers::new("", opts);
let prefixes = Prefixes::new(browsers, &PREFIXES);
```

Generic queries (`"defaults"`, `"> 1%"`) hit the oxc fallback and DRIFT.
Don't byte-test against them. AFM-shaped queries (`last N <browser> version`)
or the AFM fixture path are the only byte-clean inputs.

---

## What you must NOT do

1. Do NOT touch `supports.rs`, `transition.rs`, `processor.rs`, any
   `hacks/*.rs`, or the `register_hacks()` body. Other agents own those.
2. Do NOT edit `crates/parity-runner/src/{stages,main}.rs`,
   `packages/css/scripts/parity-bridge.mjs`, or `crates/css/src/transform.rs`.
   AGENT_6 owns those (and they require asking the user first).
3. Do NOT bump any pinned version (caniuse-lite, browserslist, postcss,
   autoprefixer, anything in `PARITY_VERSIONS.md`).
4. Do NOT "fix" upstream bugs. If `prefixes.js@10.4.14` has a bug,
   replicate it. File the bug elsewhere, move on.
5. Do NOT write your own tree walk. Use
   `postcss_core::walk_*_mut_with_parent` family.
6. Do NOT use `format!("{}", f64)` for output bytes. Use
   `postcss_core::js_number_to_string`.
7. Do NOT use `HashMap` on the hashing path. `IndexMap` only.
8. Do NOT remove `oxc_browserslist` from anywhere. Fallback is
   load-bearing for cssnano consumers.
9. Do NOT widen the AFM grammar in `browserslist-shim`. Out of scope.
10. Do NOT add new methods to base traits (`Declaration`, `Value`,
    `Selector`, `AtRule`, `Resolution`). Their shape is locked. If you
    need something missing, FILE A NOTE in your handover and pause.

---

## Sign-off gates — run all three before claiming done

```bash
cd crates
RUSTFLAGS="" cargo test -p autoprefixer        # must show ≥60 passing, 0 failing, 0 ignored
RUSTFLAGS="" cargo build -p autoprefixer       # clean
RUSTFLAGS="" cargo check --workspace           # clean (catches cross-crate breakage)
```

If any of these fail and you can't fix them in 10 minutes, ROLL BACK
your changes with `git restore` and document what you tried in your
handover. Better an honest "I tried this approach and rolled back"
than a half-landing that silently regresses the floor.

---

## What to write when done

Write `crates/autoprefixer/AGENT_1_DONE.md` with:
- Test count delta (from 60 to N).
- File-by-file summary of what you changed.
- Any JS quirks you discovered (add them to a "found quirks" list — the
  controller agent will fold them into HANDOVER §11).
- Any base-class methods you wished existed but couldn't add (so
  AGENT_4 / AGENT_5 know).
- Anything you DIDN'T finish that was in your scope, and why.
- The exact `BrowsersOptions::from` pattern your tests use, so other
  agents can copy it.
- Confirm AGENT_4 is now unblocked (your `Prefixes::new` is the
  pre-condition for `processor.rs`).

Do NOT update HANDOVER.md / MORNING.md / STATUS.md yourself — the
controller agent does that based on your AGENT_1_DONE.md report. This
keeps the merge surface for the controller to one file per subagent.

---

## If you're stuck

Vendored JS at `crates/_vendor/autoprefixer-10.4.14/package/lib/`. Read
THAT, not GitHub, not Stack Overflow.

For postcss-core API questions, read `crates/postcss-core/src/`.

For the cursor-shift bug pattern (if you find yourself inserting in a
loop), see `crates/autoprefixer/src/at_rule.rs::process` and HANDOVER §3.

Good luck. ONE unit. 0 → 100%. Stop.
