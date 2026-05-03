# AGENT_2 — `supports.rs` (`@supports` query rewriting)

You are picking up a focused unit inside the larger `crates/autoprefixer`
port. You have NO memory of the prior conversation — this file plus the
docs it points at are your full briefing.

You can run in parallel with AGENT_3 (transition.rs). You depend on
AGENT_1's `Prefixes::new` for the data-shape of the prefixes table —
specifically the methods `Prefixes::add` and `Prefixes::remove` — but
ONLY for shape, not behavior. If AGENT_1 hasn't finished, you can still
write your code against the locked signature in `prefixes.rs` and unit-
test against mock `Prefixes` instances.

---

## What you own

ONE unit, taken 0 → 100% byte-clean against the JS oracle:

**`crates/autoprefixer/src/supports.rs`** — currently a 9-line stub.
Maps to `crates/_vendor/autoprefixer-10.4.14/package/lib/supports.js`
(302 LOC). Rewrites `@supports` rule prelude conditions to add vendor
prefixes for the queried features.

Example transformation (JS oracle):
```css
/* input */
@supports (display: flex) {
  .x { color: red; }
}

/* output (with -webkit- target browser) */
@supports ((display: -webkit-flex) or (display: flex)) {
  .x { color: red; }
}
```

That's the whole unit. Do NOT touch anything else.

---

## Read these BEFORE writing code (in this order)

1. `crates/PARITY_VERSIONS.md` — byte-equality contract. ~5 min.
2. `crates/PLUGIN_IMPLEMENTATION_GUIDE.md` — AST surface, helpers.
   ~10 min.
3. `crates/autoprefixer/HANDOVER.md` —
   - §1 (current floor — do not regress)
   - §3 (cursor-shift bug — supports inserts new at-rule prefix variants)
   - §6 (browserslist gate CLOSED, test discipline)
   - §11 (JS quirks)
   ~15 min.
4. `crates/autoprefixer/GATE_CLOSED_FOR_AUTOPREFIXER_AGENT.md` ~5 min.
5. **The vendored JS source** —
   `crates/_vendor/autoprefixer-10.4.14/package/lib/supports.js` lines
   1–302. End-to-end. ~15 min.
6. The vendored upstream tests at
   `crates/_vendor/autoprefixer-10.4.14/package/test/__tests__/supports.test.js`
   if it exists, otherwise grep the autoprefixer test suite for
   `@supports` for input/expected pairs. ~10 min — copy these test cases
   to your Rust unit tests.

Total: ~60 min reading.

---

## Where things stand

`cargo test -p autoprefixer` → **60 passing, 0 ignored**. Floor; do not
regress.

What's REAL (you depend on these):
- All base classes: `Prefixer`, `AtRuleBase`, `ValueBase`, `SelectorBase`,
  `DeclarationBase`, `ResolutionBase`. Full bodies, real tests.
- `Browsers` (caniuse-db agents + browserslist-shim hybrid resolver).
- `data/prefixes.rs` (183-entry static table, byte-clean).
- `prefixes.rs::HackRegistry` skeleton.

What's STUBBED:
- `Prefixes::new` etc. — AGENT_1 (you depend on its signature, NOT body).
- `transition.rs` — AGENT_3 (parallel to you).
- `processor.rs` — AGENT_4.
- All hacks — AGENT_5.

---

## The `Supports` class API in JS

Reading `supports.js`, the public surface is:

- `new Supports(prefixes)` — constructor takes the parent `Prefixes`.
- `prefixer(name)` — returns the right `Prefixer` subclass for a name
  (`Declaration` / `Value` / `Selector` / `AtRule`). Reads from the
  hack registry + base class lookup.
- `cleanBrackets(node)` — recursively strip nested unnecessary parens
  in the `@supports` AST.
- `convert(progress)` — turn an `@supports` query AST into a normalized
  shape.
- `normalize(nodes)` — sort + uniq an array of conditions.
- `add(declParts, all)` — for one `(prop: value)` clause, expand to all
  prefix-equivalent clauses joined by `or`.
- `process(rule)` — entry point. Walks `rule.params` (the query string),
  parses via `crates/_vendor/autoprefixer-10.4.14/package/lib/brackets.js`
  (already ported in `crates/autoprefixer/src/brackets.rs` — use it),
  rewrites, re-stringifies.

`process(rule)` is what `processor.rs` (AGENT_4) calls into.

Map each JS method to a Rust function. Same file layout, same names
(snake_case). Mirror the upstream signature with `&Browsers` /
`&Prefixes` borrows where JS used `this.all`.

---

## Test discipline — DO NOT rely on cwd

Per HANDOVER §6: every test that uses `Browsers::new(...)` MUST set
`BrowsersOptions::from` explicitly. See the snippet in AGENT_1.md "Test
discipline" section, or the `workspace_root()` helper in
`crates/autoprefixer/tests/browserslist_parity.rs`.

For a unit test that doesn't actually need browser-specific output (a
lot of the parsing tests don't), feed `Supports` a hand-constructed
`Prefixes` with a fixed `add_table` instead of going through
`Browsers::new` at all. Mock the upstream layer; pin the byte-output of
the unit you're testing.

For tests that DO go through `Browsers::new`, use the AFM fixture path
(`crates/browserslist-shim/tests/fixtures/afm`).

DO NOT byte-test against `defaults` / `> 1%` / `chrome >= 50` etc. —
those hit the oxc fallback and drift.

---

## Cursor-shift bug — read HANDOVER §3

If `supports.rs::process` ever inserts new at-rule prefix variants in a
loop (it MIGHT — check the JS), you need the path-bump pattern from
`at_rule.rs::process`. Heuristic: any test that exercises ≥2 prefixes
for the same `@supports` rule MUST verify all clones land. A single-
prefix test is silent on this bug.

If `supports.rs` only mutates `rule.params` in place (no inserts), this
doesn't apply.

---

## What you must NOT do

1. Do NOT touch `transition.rs`, `processor.rs`, `prefixes.rs::Prefixes`
   bodies, any `hacks/*.rs`. Other agents own those.
2. Do NOT edit `crates/parity-runner/`, `packages/css/`, or
   `crates/css/src/transform.rs`. Out of scope.
3. Do NOT bump any pinned version.
4. Do NOT "fix" upstream bugs. Replicate them.
5. Do NOT write your own tree walk. `postcss_core::walk_*_mut_with_parent`.
6. Do NOT use `format!("{}", f64)`. Use `postcss_core::js_number_to_string`.
7. Do NOT use `HashMap`. `IndexMap` only.
8. Do NOT add new methods to base traits. If you need something missing,
   FILE A NOTE in your handover and pause.
9. Do NOT roll your own bracket parser. Use
   `crates/autoprefixer/src/brackets.rs` (already ported, has the same
   semantics as the JS `brackets.js`).
10. Do NOT remove `oxc_browserslist` or widen the AFM grammar — out of
    scope.

---

## Sign-off gates

```bash
cd crates
RUSTFLAGS="" cargo test -p autoprefixer        # must show ≥60 passing, 0 failing, 0 ignored
                                               #   (more if your unit tests pass)
RUSTFLAGS="" cargo build -p autoprefixer       # clean
RUSTFLAGS="" cargo check --workspace           # clean
```

If anything fails and you can't fix in 10 min, ROLL BACK.

---

## What to write when done

Write `crates/autoprefixer/AGENT_2_DONE.md` with:
- Test count delta.
- File-by-file summary.
- JS quirks discovered (the controller will fold these into HANDOVER §11).
- Any base-class methods you wished existed.
- Anything you didn't finish, and why.
- Whether you used the AFM fixture or hand-mocked `Prefixes` for tests.

Do NOT update HANDOVER.md / MORNING.md / STATUS.md yourself.

---

## If you're stuck

Vendored JS at `crates/_vendor/autoprefixer-10.4.14/package/lib/supports.js`.
Vendored test inputs in the same package's `test/` directory.
HANDOVER.md `crates/autoprefixer/` is exhaustive.

ONE unit. 0 → 100%. Stop.
