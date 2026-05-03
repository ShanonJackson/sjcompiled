# AGENT_4 — `processor.rs` (the main walk, the engine)

You are picking up THE largest single unit inside the larger
`crates/autoprefixer` port. You have NO memory of the prior conversation
— this file plus the docs it points at are your full briefing.

---

## What you own

ONE unit, taken 0 → 100% byte-clean against the JS oracle:

**`crates/autoprefixer/src/processor.rs`** — currently a stub.
Maps to `crates/_vendor/autoprefixer-10.4.14/package/lib/processor.js`
(~720 LOC). The main tree walk that orchestrates every base class +
hack to add/remove prefixes across an entire stylesheet.

This is **THE engine**. Without it, autoprefixer cannot prefix a single
line of CSS end-to-end.

This unit is realistically 2–3 sessions on its own. You may not finish
in one. If you can't, finish whatever sub-piece you can take 0→100% and
hand off cleanly; do NOT half-land.

---

## Hard pre-conditions — do not start without these green

1. **AGENT_1 must have landed `Prefixes::new` + `cleaner` + `select` +
   `group`.** Check that `crates/autoprefixer/AGENT_1_DONE.md` exists
   and `cargo test -p autoprefixer` shows the new test count from that
   agent.
2. AGENT_2 (`supports.rs`) and AGENT_3 (`transition.rs`) ideally
   landed too, since `processor.rs` calls into both. If they haven't,
   you can stub the calls into `Supports::process` and `Transition::process`
   for now and let the test suite cover them when those agents finish.

If pre-condition 1 isn't met: STOP. Don't proceed. File a note that
you're blocked on AGENT_1 and exit.

---

## Read these BEFORE writing code (in this order)

1. `crates/PARITY_VERSIONS.md` — byte-equality contract. ~5 min.
2. `crates/PLUGIN_IMPLEMENTATION_GUIDE.md` — AST surface, helpers,
   `walk_*_mut_with_parent` family. ~10 min.
3. `crates/autoprefixer/HANDOVER.md` — read ALL of it. ~20 min. Pay
   special attention to:
   - §1 (current floor)
   - §3 (cursor-shift bug — this is YOUR battlefield, processor.rs is
     where it bites worst)
   - §4 (`Prefixes::group` semantics)
   - §6 (browserslist gate, test discipline)
   - §7 (`_autoprefixer*` keys, clone-strip)
   - §11 (JS quirks)
4. `crates/autoprefixer/GATE_CLOSED_FOR_AUTOPREFIXER_AGENT.md` ~5 min.
5. `crates/autoprefixer/AGENT_1_DONE.md` (whatever AGENT_1 wrote) ~5 min.
6. **The vendored JS source** —
   `crates/_vendor/autoprefixer-10.4.14/package/lib/processor.js` lines
   1–720. End-to-end. ~30 min.
7. The vendored upstream test suite at
   `crates/_vendor/autoprefixer-10.4.14/package/test/__tests__/`. The
   `autoprefixer.test.js` file is the integration test surface — its
   inputs/expected pairs are EXACTLY what you should mirror as Rust
   integration tests. ~20 min.

Total: ~95 min reading. Don't skip it. The cost of a wrong assumption
in `processor.rs` is hours of debugging.

---

## Where things stand

`cargo test -p autoprefixer` → **≥60 passing** (whatever AGENT_1/2/3
land puts it higher). Floor.

What's REAL:
- Everything AGENT_1/2/3 finishes — `Prefixes::new`, `Supports`,
  `Transition`.
- All base classes (`Prefixer`, `AtRuleBase`, `ValueBase`,
  `SelectorBase`, `DeclarationBase`, `ResolutionBase`).
- `Browsers`, `data/prefixes.rs`, `prefixes.rs::HackRegistry`.

What's STUBBED:
- All 58 hacks — AGENT_5 (you do NOT depend on hacks landing for
  `processor.rs` to compile and unit-test on the AFM surface; the hack
  registry is empty and the base classes handle the no-hack-registered
  case).
- `Stage::Autoprefixer` parity-runner stage + NAPI wire-in — AGENT_6.

---

## The shape of `processor.rs`

Reading `processor.js`, the public surface is essentially:

- `new Processor(prefixes)` — constructor.
- `add(css, result)` — main entry. Walks the entire stylesheet, calling
  `Prefixes`-driven add/remove on every node.
- `remove(css, result)` — the cleanup pass; uses `Prefixes::cleaner`.
- `prefix(decl)`, `withHackUtilization(...)` etc. — internal helpers.

The walk goes node-by-node:
- For each `Rule`, call `Selector::process(rule, prefixes)` and
  `Declaration::process(decl, prefixes)` for each child decl.
- For each `AtRule`, call `AtRule::process(at_rule, prefixes)`.
- For each `Declaration`, the standard `Declaration::process` chain
  (which walks values via `Value::process` etc.).
- For `@supports`, dispatch to `Supports::process`.
- For `transition` decls, dispatch to `Transition::process`.

This is conceptually one big `walk_mut_with_parent` over the root, but
the dispatch logic (Rule vs AtRule vs Declaration) is non-trivial and
must mirror `processor.js` line-for-line.

---

## The cursor-shift bug WILL bite you

It already bit two earlier base-class ports. `processor.rs` is the
worst place for it because the walk holds paths across MANY insert
operations.

Mitigations:
1. **Use `postcss_core::walk_*_mut_with_parent` family.** Their
   `DeferredMutation` queue handles the cursor-shift internally — but
   only IF you use the queue and don't call `insert_before_at_path`
   yourself outside the closure.
2. **For inserts that DO go through `insert_before_at_path` directly
   (e.g., from inside a base-class `add` method, called from your
   walker), the base class is already cursor-correct.** Your job is to
   not work around it.
3. **Heuristic test**: any test that exercises ≥2 prefixes per node
   MUST verify all clones land. Single-prefix tests are silent.

Read HANDOVER §3 again before writing the walk loop.

---

## Test discipline — DO NOT rely on cwd

Per HANDOVER §6: explicit `BrowsersOptions::from`. See AGENT_1.md "Test
discipline" for the pattern.

For end-to-end integration tests, use the AFM `.browserslistrc` fixture
path. Mirror autoprefixer's upstream `__tests__/autoprefixer.test.js`
input/expected pairs as your Rust integration tests, but only for the
inputs whose expected output is reachable on AFM-shaped queries.

If you find a test case in upstream that uses `defaults` / `> 1%` etc.,
either skip it (note in handover) or hand-curate `selected` to bypass
`Browsers::new`.

---

## Scope discipline — descope if you have to

If `processor.rs` is too big for your session, take ONE coherent slice
0→100% byte-clean. Examples of slice-able sub-units:

- Just the `add` pass (skip `remove` — handle in next session).
- Just the at-rule walk (skip declaration walk — handle in next session).
- The full walk skeleton with hack-dispatch stubbed to no-op (covers
  AFM's surface today, since hacks haven't landed).

Land what you finish, document what you didn't, hand off. Better to
land 30% byte-clean than 100% byte-fragile.

---

## What you must NOT do

1. Do NOT touch any `hacks/*.rs`. AGENT_5 owns those.
2. Do NOT edit `crates/parity-runner/`, `packages/css/`, or
   `crates/css/src/transform.rs`. AGENT_6 owns those.
3. Do NOT bump any pinned version.
4. Do NOT "fix" upstream bugs. Replicate them.
5. Do NOT write your own tree walk. Use
   `postcss_core::walk_*_mut_with_parent` family.
6. Do NOT use `format!("{}", f64)`. Use `postcss_core::js_number_to_string`.
7. Do NOT use `HashMap`. `IndexMap` only.
8. Do NOT add new methods to base traits. If you need something missing,
   FILE A NOTE in your handover and pause. Other agents wrote those
   shapes against your eventual call sites — changing them silently
   breaks them.
9. Do NOT remove `oxc_browserslist` or widen the AFM grammar.
10. Do NOT modify AGENT_1's `Prefixes::new` body. If you find a bug,
    file it. The fix landing in YOUR session would force AGENT_1 to
    re-test.

---

## Sign-off gates

```bash
cd crates
RUSTFLAGS="" cargo test -p autoprefixer        # must show ≥(prior floor) passing, 0 failing, 0 ignored
RUSTFLAGS="" cargo build -p autoprefixer       # clean
RUSTFLAGS="" cargo check --workspace           # clean
```

If anything fails and you can't fix in 10 min, ROLL BACK.

If you only landed a slice (per "Scope discipline" above), the floor
must STILL not regress — your unfinished code paths must compile and
not panic when not exercised.

---

## What to write when done

Write `crates/autoprefixer/AGENT_4_DONE.md` with:
- Test count delta.
- File-by-file summary.
- Which slice(s) landed; which deferred (and rationale).
- JS quirks discovered.
- Any cursor-shift bugs you hit and the path-bump pattern that fixed them.
- Whether AGENT_5 (hacks) is now unblocked. (Yes if your walk dispatches
  through `HackRegistry::lookup`. No if you stubbed that path — explain.)
- Whether AGENT_6 (NAPI wire-in) is unblocked. (Yes only if your engine
  works end-to-end through the AFM fixture path.)

Do NOT update HANDOVER.md / MORNING.md / STATUS.md yourself.

---

## If you're stuck

Vendored JS at `crates/_vendor/autoprefixer-10.4.14/package/lib/processor.js`.
Vendored test inputs in `package/test/__tests__/`.

For the cursor-shift pattern, read `at_rule.rs::process` and HANDOVER §3.
For the `walk_*_mut_with_parent` API, read `crates/postcss-core/src/`.
For each base class's `process` signature, read its file in
`crates/autoprefixer/src/`.

ONE unit. 0 → 100%. Or one slice 0 → 100% if the unit is too big. Stop.
