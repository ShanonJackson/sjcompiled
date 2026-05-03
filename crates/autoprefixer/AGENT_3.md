# AGENT_3 — `transition.rs` (transition shorthand handling)

You are picking up a focused unit inside the larger `crates/autoprefixer`
port. You have NO memory of the prior conversation — this file plus the
docs it points at are your full briefing.

You can run in parallel with AGENT_2 (supports.rs). You depend on
AGENT_1's `Prefixes` data shape but only for signature; mock-test
locally if AGENT_1 hasn't finished.

---

## What you own

ONE unit, taken 0 → 100% byte-clean against the JS oracle:

**`crates/autoprefixer/src/transition.rs`** — currently a 9-line stub.
Maps to `crates/_vendor/autoprefixer-10.4.14/package/lib/transition.js`
(329 LOC). Handles the `transition` shorthand declaration: when the
value contains a property name (e.g., `transform 0.3s`), the transition
needs prefix-matched siblings on the declaration.

Example transformation (JS oracle, with `-webkit-` target):
```css
/* input */
.x { transition: transform 0.3s ease; }

/* output */
.x {
  -webkit-transition: -webkit-transform 0.3s ease;
  transition: transform 0.3s ease;
}
```

That's the whole unit. Do NOT touch anything else.

---

## Read these BEFORE writing code (in this order)

1. `crates/PARITY_VERSIONS.md` — byte-equality contract. ~5 min.
2. `crates/PLUGIN_IMPLEMENTATION_GUIDE.md` — AST surface, helpers.
   ~10 min.
3. `crates/autoprefixer/HANDOVER.md` —
   - §1 (current floor)
   - §3 (cursor-shift bug — transition does insert-in-loop)
   - §6 (browserslist gate CLOSED, test discipline)
   - §7 (`_autoprefixer*` attribute keys)
   - §11 (JS quirks)
   ~15 min.
4. **The vendored JS source** —
   `crates/_vendor/autoprefixer-10.4.14/package/lib/transition.js` lines
   1–329. End-to-end. ~15 min.
5. The vendored upstream test cases for transition (grep test/ for
   `transition`). ~10 min — copy as Rust unit tests.
6. `crates/autoprefixer/src/declaration.rs` — your `Transition` struct
   composes/wraps `Declaration`. Read the declaration.rs API surface.
   ~10 min.

Total: ~65 min.

---

## Where things stand

`cargo test -p autoprefixer` → **60 passing, 0 ignored**. Floor.

What's REAL:
- All base classes (`Prefixer`, `AtRuleBase`, `ValueBase`, `SelectorBase`,
  `DeclarationBase`, `ResolutionBase`).
- `Browsers`, `data/prefixes.rs`, `prefixes.rs::HackRegistry`.
- `crates/autoprefixer/src/old_value.rs`, `old_selector.rs`,
  `vendor.rs` — these have helpers transition.js uses.

What's STUBBED:
- `Prefixes::new` etc. — AGENT_1.
- `supports.rs` — AGENT_2 (parallel to you).
- `processor.rs` — AGENT_4.
- All hacks — AGENT_5.

---

## The `Transition` class API in JS

Reading `transition.js`, the public surface is:

- `new Transition(prefixes)` — constructor takes the parent `Prefixes`.
- `add(decl, result)` — entry point called by `processor.js` for each
  `transition` decl. Walks the value tokens via
  `postcss-value-parser`, finds property-name tokens, generates
  prefix-matched sibling decls.
- `findProp(decl)` — extract the property name from a transition value.
- `isPrefixed(prop)`, `otherPrefixes(value, prop)`, `cloneBefore(decl, prop, value)` —
  utility methods used inside `add`.
- `remove(decl)` — strip stale prefix variants.
- `process(decl)` — top-level orchestration.

Map each JS method to a Rust function. Same file, same names.

---

## Cursor-shift bug — read HANDOVER §3

`transition.rs::add` will call `parent.insertBefore(decl, cloned)` in a
loop (one insert per added prefix). The path becomes stale every
iteration. Use the bump pattern from `at_rule.rs::process`:

```rust
let mut current_path = path.to_vec();
for prefix in &prefixes {
    if self.add(root, &current_path, prefix).is_some() {
        if let Some(last) = current_path.last_mut() { *last += 1; }
    }
}
```

Heuristic: write a test that exercises ≥2 prefixes for the same decl.
Single-prefix tests are silent on this bug.

---

## `Node.attrs` keys

If you cache state on a node (transition.js does this for the
`_autoprefixerCascade` flag etc.), namespace your key with
`_autoprefixer` and ADD it to `prefixer::CLONE_STRIP_KEYS` in
`crates/autoprefixer/src/prefixer.rs`. Otherwise clones will inherit
stale memos and silently break parity.

Existing keys (don't conflict): `_autoprefixerPrefix`,
`_autoprefixerValues`, `_autoprefixerCascade`, `_autoprefixerMax`,
`_autoprefixerPrefixeds`, `proxyCache`.

---

## Test discipline — DO NOT rely on cwd

Per HANDOVER §6: explicit `BrowsersOptions::from`. See AGENT_1.md "Test
discipline" section for the exact pattern.

For pure value-parsing tests (no browser involvement), mock `Prefixes`
directly.

---

## What you must NOT do

1. Do NOT touch `supports.rs`, `processor.rs`, `prefixes.rs::Prefixes`
   bodies, any `hacks/*.rs`. Other agents own those.
2. Do NOT edit `crates/parity-runner/`, `packages/css/`, or
   `crates/css/src/transform.rs`. Out of scope.
3. Do NOT bump any pinned version.
4. Do NOT "fix" upstream bugs.
5. Do NOT write your own tree walk. `postcss_core::walk_*_mut_with_parent`.
6. Do NOT use `format!("{}", f64)`. Use `postcss_core::js_number_to_string`.
7. Do NOT use `HashMap`. `IndexMap` only.
8. Do NOT add new methods to base traits. If you need something missing,
   FILE A NOTE in your handover and pause.
9. Do NOT roll your own value parser. Use
   `crates/postcss-value-parser/`.
10. Do NOT remove `oxc_browserslist` or widen the AFM grammar.

---

## Sign-off gates

```bash
cd crates
RUSTFLAGS="" cargo test -p autoprefixer        # must show ≥60 passing, 0 failing, 0 ignored
RUSTFLAGS="" cargo build -p autoprefixer       # clean
RUSTFLAGS="" cargo check --workspace           # clean
```

If anything fails and you can't fix in 10 min, ROLL BACK.

---

## What to write when done

Write `crates/autoprefixer/AGENT_3_DONE.md` with:
- Test count delta.
- File-by-file summary.
- JS quirks discovered.
- Any base-class methods you wished existed.
- Anything you didn't finish, and why.
- Confirm whether you needed to touch `prefixer::CLONE_STRIP_KEYS`
  (and what key you added).

Do NOT update HANDOVER.md / MORNING.md / STATUS.md yourself.

---

## If you're stuck

Vendored JS at `crates/_vendor/autoprefixer-10.4.14/package/lib/transition.js`.
HANDOVER.md is exhaustive.

ONE unit. 0 → 100%. Stop.
