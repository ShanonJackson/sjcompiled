# Autoprefixer port — Handover

**Read this first.** It captures what `STATUS.md` / `HACKS_PORT.md` /
`PLUGIN_IMPLEMENTATION_GUIDE.md` *don't* say — the unwritten gotchas, the
shortcut-that-isn't-a-shortcut, and the verification gates you must
actually run before claiming a piece is byte-clean.

---

## 1. Where you actually are

`cargo test -p autoprefixer` → **56 passing** (52 unit + 4 parity).
That number is the floor; your work must keep it there or grow it.
Run it before EVERY commit.

What's real (full bodies, real tests):
- `utils.rs`, `vendor.rs`, `brackets.rs`, `old_value.rs`,
  `old_selector.rs`
- `prefixer.rs` (parent walk + cache + clone-strip)
- `browsers.rs` (caniuse-db agents + browserslist-shim)
- `at_rule.rs` / `value.rs` / `selector.rs` / `declaration.rs` /
  `resolution.rs` (full base classes)
- `prefixes.rs` (HackRegistry skeleton; orchestrator body stubbed)
- **`data/prefixes.rs`** — 183 entries, codegen via `build.rs` + `bun`,
  4 parity gates byte-clean. See `crates/STATUS.md` "Phase 7 ship —
  `data/prefixes.rs` codegen + caniuse-lite pin fix" for the full
  story (including the workspace caniuse-lite pin fix that landed
  alongside it).

What's stubbed (`unimplemented!()`):
- `supports.rs`, `transition.rs`
- `Prefixes` orchestrator methods inside `prefixes.rs`
  (`new` / `cleaner` / `select` / `group` — depends on `data/prefixes.rs`
  ✅ and `processor.rs` ⬜)
- `processor.rs`, `info.rs`, `autoprefixer.rs`
- All 58 hack files (parallel agent's territory)

---

## 2. `data/prefixes.rs` is DONE — read this if you're regenerating

If you only need to know the table is byte-clean, see `crates/STATUS.md`
"Phase 7 ship — `data/prefixes.rs` codegen + caniuse-lite pin fix" and
move on. This section is for the next agent who has to MAINTAIN the
codegen (e.g. caniuse-lite version bump, table shape change).

### Shape

`PrefixEntry` (in `src/data/prefixes.rs`) mirrors the JS object shape:
`browsers` (Vec<String>), `feature` (Option<String>), `mistakes`/`props`
(Vec<String>, optional), `selector`/`transition` (bool, optional).
`#[serde(skip_serializing_if = ...)]` matches JS's "omit-when-falsy"
convention so the parity test's canonical-JSON compare lines up.

### Codegen pipeline

`build.rs` → spawns `bun <file>` (Windows shim-aware via the
`bun`/`bun.cmd`/`bun.exe` candidates fallback) on a tmp script that
`require()`s `crates/_vendor/autoprefixer-10.4.14/package/data/prefixes.js`
→ dumps the resulting object as JSON → parses via `serde_json` with
`preserve_order` (load-bearing) → emits N `m.insert(...)` statements
wrapped in a single block expression at `$OUT_DIR/prefixes_table.rs`
→ `src/data/prefixes.rs` `include!`s the block inside the `Lazy::new`
closure.

### Pre-conditions

- `bun` on PATH.
- `bun install` has run at workspace root with `caniuse-lite:
  1.0.30001690` in BOTH root `package.json` `overrides` AND
  `devDependencies` (the latter is load-bearing — without the direct
  devDep, bun's isolated install layout leaves no top-level
  `node_modules/caniuse-lite/` symlink and the vendored JS resolves a
  parent-directory shadow).
- `build.rs` panics with a clear message if either pre-condition fails.

### If you need to bump caniuse-lite

DON'T. Anomaly #3 in `PARITY_VERSIONS.md` says it's frozen at
1.0.30001690 forever. If a future session genuinely needs a bump,
the work order is:

1. Update `caniuse-lite` version in root `package.json` `overrides`
   AND `devDependencies` (both must match).
2. `bun install`.
3. `cargo build -p autoprefixer && cargo test -p autoprefixer`.
4. The `caniuse_lite_pin_matches_parity_versions` test will fail —
   update the asserted version to match.
5. The `data_table_matches_js_oracle` test will likely still pass
   (both sides regenerated from the same lite). That's the wrong
   gate for a version bump — it can't catch this class of change.
   Run a full corpus diff against the previous Rust output instead,
   and accept only if the diff is purely additive (new browser
   versions for existing entries, no key removals).
6. Update `PARITY_VERSIONS.md` Anomaly #3, this file, STATUS.md.
7. Brace for downstream hash rotations across every consumer.

---

## 3. The cursor-shift bug WILL bite you in `processor.rs`

It already bit me twice — once in `at_rule.rs::process`, once in
`declaration.rs::process`. Both fixed (see STATUS.md "Path-shift
gotcha"). The third place it will bite you is in `processor.rs`'s
main walk, because that file calls `parent.insertBefore(node, cloned)`
indirectly through every base class's `add` method, in a loop, while
holding a path.

The fix pattern is in `at_rule.rs::process`:

```rust
let mut current_path = path.to_vec();
for prefix in &prefixes {
    if self.add(root, &current_path, prefix).is_some() {
        if let Some(last) = current_path.last_mut() { *last += 1; }
    }
}
```

**Heuristic for catching it in your own code:** any test that exercises
≥2 prefixes for the same node MUST verify all clones land. A single-
prefix test is silent on this bug.

`processor.rs::walk` has yet another wrinkle: it walks the WHOLE tree,
and inserts at any depth shift indices for ALL paths queued behind the
walk cursor. The postcss-core `walk_*_mut_with_parent` family handles
this internally (the `DeferredMutation` queue), so as long as
`processor.rs` uses those rather than rolling its own walk, you're
covered. **Do not roll your own walk.**

---

## 4. `Prefixes::group(decl)` — what it actually means

`declaration.js::restoreBefore` calls `this.all.group(decl).up(...)`.
This is the JS `Prefixes::group` method (in `prefixes.js`). It
returns an iterator-like object that walks "the group of adjacent
prefixed declarations around `decl`". Specifically, JS `up(callback)`
walks BACKWARDS through the decl's siblings, calling `callback` on
each prefixed-equivalent decl until either the callback returns
truthy (= match found) or a non-prefixed-equivalent sibling is hit.

In our path-based world, this maps to:

```rust
// In prefixes.rs::Prefixes::group(decl):
// Build a view that walks backwards from `decl`'s path using
// `sibling_relative(root, path, -1)`, `(-2)`, ... and yields each
// adjacent decl whose normalized prop matches `vendor::unprefixed(decl.prop)`.
```

`Selector::already` already implements the same shape — read it for
the pattern (`crates/autoprefixer/src/selector.rs::already`). The only
difference is `Selector` walks rule siblings; `Prefixes::group` walks
decl siblings inside the same Rule.

I left `restore_before` as a no-op pending this. Once `Prefixes::group`
lands, fill it in — the cascade tests in `__tests__/cascade.test.js`
will fail visibly if you forget.

---

## 5. The `caniuse-lite` snapshot is frozen at 1.0.30001690

`crates/caniuse-db/` reads its data from a build-time JSON snapshot
that's pinned to that version (see `PARITY_VERSIONS.md` Anomaly #3).
**Never let it auto-update.** Autoprefixer's prefix decisions for
"is browser X supported?" depend on this snapshot. A drift here = silent
hash rotation.

If you ever see `caniuse-db`'s build script try to fetch fresh data,
that's a regression — file it to the caniuse-db agent.

---

## 6. `browserslist-shim` defaults are a known weak spot

`browserslist-shim::index::resolve_with` wraps `oxc_browserslist`. The
default query is locked to browserslist@4.24.4's
`["> 0.5%", "last 2 versions", "Firefox ESR", "not dead"]` — but the
*evaluation* of that query depends on the underlying `oxc_browserslist`
crate's behavior, which may have its own drift.

**Verification gate before claiming `Browsers::new(...)` works
correctly:** run a JS oracle that does `browserslist(...)` for a few
canonical queries (`"defaults"`, `"> 1%"`, `"chrome >= 50"`) and assert
the Rust output matches the JS array element-by-element. The
cssnano agents have a similar gate; copy their pattern from
`crates/parity-runner/src/stages.rs`.

If JS and Rust diverge on the default query, that's a `browserslist-shim`
bug. Don't paper over it inside `Browsers`.

---

## 7. The `_autoprefixer*` attribute keys

I namespaced everything with `_autoprefixer` to avoid colliding with
other plugins. Constants live next to the consuming module:

| Key                          | File                     | Variant                      |
|------------------------------|--------------------------|------------------------------|
| `_autoprefixerPrefix`        | `prefixer.rs`            | `Bool(false)` or `String(p)` |
| `_autoprefixerValues`        | `value.rs`               | `StringMap`                  |
| `_autoprefixerPrefixeds`     | `selector.rs`            | `NestedStringMap`            |
| `_autoprefixerCascade`       | `declaration.rs`         | `Bool`                       |
| `_autoprefixerMax`           | `declaration.rs`         | `Int`                        |
| `proxyCache`                 | (referenced in JS clone) | not used in Rust port        |

`prefixer::CLONE_STRIP_KEYS` lists all of these. If you add a new key,
**add it to that list** — `prefixer::clone_node` calls
`Node::clone_without(CLONE_STRIP_KEYS)` and a clone with stale memos
silently breaks parity.

`Node::clone_without` is RECURSIVE per the postcss-core agent's
regression test. You don't have to walk children yourself.

---

## 8. The four shared files I touched

These are workspace-shared. Future edits should ASK first (per the
session-start agreement):

| File                           | What I added                                |
|--------------------------------|---------------------------------------------|
| `crates/Cargo.toml`            | `"autoprefixer"` member + workspace dep    |
| `crates/STATUS.md`             | Phase 7 split contract section + state     |
| `crates/PLUGIN_IMPLEMENTATION_GUIDE.md` | Untouched by me — postcss-core agent updated |
| `crates/autoprefixer/src/prefixes.rs::register_hacks` | Empty BEGIN/END block; hacks agent appends here |

When you wire the parity-runner stage in, three more shared files come
into play:
- `crates/parity-runner/src/stages.rs` (add `Stage::Autoprefixer`)
- `crates/parity-runner/src/main.rs` (CLI mapping)
- `packages/css/scripts/parity-bridge.mjs` (JS oracle counterpart)

Forgetting the JS-bridge side produces "no diff" output that LOOKS
green because both sides hit the unknown-stage error path. STATUS.md's
Phase 8a section flagged this — read it.

---

## 9. The corpus you'll need

Phase 7 ships when `parity-runner --stage autoprefixer` is byte-clean
across a real corpus. Suggested seed (~30–50 entries):

- Each `Browsers.prefixes()` value gets at least one fixture exercising
  it (5–6 entries minimum).
- `display: flex`, `display: grid` — these are the most consequential
  decls in real atomic CSS.
- `@keyframes` (at-rule prefix path).
- `@supports (...)` (selector inside `@supports` is a known wrinkle).
- `transition: transform 0.3s` (transition.js is the heaviest hack
  outside gradient).
- `linear-gradient(...)` — gradient.js is 448 LOC, the single largest
  hack. Catches a lot of value.js bugs.
- `:fullscreen`, `::placeholder`, `::file-selector-button` (selector
  hacks).
- A "no-op" fixture per browser query (e.g. `last 1 chrome version` →
  no prefixing happens) — proves the negative case.
- An input that mixes already-prefixed + unprefixed in the same rule
  (catches `isAlready` + `otherPrefixes` interaction).

The corpus design itself is non-trivial; budget time for it.

---

## 10. What you do NOT need to port

`bin/autoprefixer` — the CLI binary. Not on the hashing path; do not
port. Same for `lib/info.js` content beyond the bare module shell — it
exists for `info()` diagnostics that aren't reachable from
`transformCss`'s pipeline.

The `autoprefixer.d.ts` file is TypeScript declarations. Skip it
entirely.

---

## 11. Subtle JS quirks I had to recreate

These are the kind of things JS-vs-Rust drift hides in. Keep the list
growing as you find more:

- `".x".split(/(?=\.|#)/g)` returns `[".x"]`, NOT `["", ".x"]` — V8
  suppresses leading empties on lookahead-at-position-0.
  Documented in `utils::split_on_class_id` test.
- `Browsers.prefixes()` is sorted by descending length, **stable on
  ties**. JS V8 TimSort is stable; Rust `sort_by` is stable; equal-
  length entries keep first-seen order from `uniq`. If you ever
  switch to an unstable sort you'll break the oracle.
- Fraction.js's default `toString()` emits decimal, NOT `n/d` —
  except autoprefixer's `-o-` resolution path which builds `n/d`
  manually. Don't switch to `fraction.to_fraction(false)` — that
  emits `"1 1/2"` mixed-number form, which JS doesn't.
- `decl.raws.before` is `Option<String>` in postcss-core. JS treats
  it as `string` with default `''`. Use `.as_deref().map(...).unwrap_or(false)`
  for boolean predicates and `.clone().unwrap_or_default()` for
  string ops. Direct field access will not compile.
- The regex `(^|[\s,(])(name($|[\s(,]))` does NOT include `;`. So
  bare-value strings like `"flex"` match, but `"display: flex;"`
  does not. The JS pipeline always passes `decl.value` (no trailing
  `;`), so this doesn't matter in production — but it WILL trip up
  unit tests that pass full decl strings to `Value::replace`.

---

## 12. The single highest-leverage thing left

**`data/prefixes.rs` is now ✅.** The next-highest-leverage unit is
`Prefixes::new` (the `prefixes.js` constructor body inside
`crates/autoprefixer/src/prefixes.rs`). It consumes the now-byte-clean
`PREFIXES` table to build the per-session `add_table` / `remove_table`
maps that `processor.rs` walks during the main pass. Without it,
`processor.rs` can't be ported. With it, `processor.rs` becomes the
next session's unit.

After `Prefixes::new`, the order is roughly:

1. **`Prefixes::new` body** (this file `prefixes.rs::Prefixes` impl —
   1 session, depends on `data/prefixes.rs` ✅ and `Browsers` ✅).
   Fill in `cleaner` / `select` / `group` at the same time since
   they share the constructor's data shape.
2. **`processor.rs`** (~720 LOC main walk — 2-3 sessions).
3. **`supports.rs`** (302 LOC `@supports` rewriting — 1-2 sessions,
   depends on `Prefixes::new`).
4. **`transition.rs`** (329 LOC transition shorthand — 1-2 sessions).
5. **`info.rs` + `autoprefixer.rs` entry shell** (1 session).
6. **Parity-runner stage + JS bridge counterpart** (re-ask user
   permission per HANDOVER §8 — touches shared files).
7. **NAPI wire-in to `transform.rs`** (re-ask permission).

A pre-Prefixes::new stretch that's also high-leverage: the
**browserslist-shim parity gate** (HANDOVER §6 — still open).
`Prefixes::new` consumes `Browsers::new(query, ignore_unknown)`; if
`browserslist-shim::resolve` diverges from JS for the canonical
queries (`"defaults"`, `"> 1%"`, `"chrome >= 50"`,
`"last 2 versions"`, `"Firefox ESR"`, `"not dead"`),
`Prefixes::new`'s output drifts silently. Closing that gate first is
~half a session and prevents the "ported `Prefixes::new` looks right
in unit tests but disagrees with JS oracle on real browser queries"
trap.

---

## 13. Final sanity check before you sign off your session

Same gates:

```bash
RUSTFLAGS="" cargo test -p autoprefixer            # must show >=56 passing
RUSTFLAGS="" cargo check -p autoprefixer           # no warnings on autoprefixer
```

If you wired a parity-runner stage:

```bash
cd packages/css
bun run scripts/parity-bridge.mjs --stage autoprefixer
cargo run -p parity-runner -- --stage autoprefixer --corpus crates/parity-runner/corpus/autoprefixer
```

Both must be 100% byte-clean. If they aren't, **don't commit "progress"**
— roll back to the last green state and document what you tried.

---

## 14. If you have to push back on scope

The full Phase 7 port is 8+ weeks for one engineer per the original
estimate in `STATUS.md`. We're maybe two days in. The cardinal rule
("a session takes a unit 0 → 100% byte-clean") is non-negotiable —
if a session can't realistically finish a unit, **scope down and
finish a smaller unit cleanly** rather than half-landing a big one.

Smaller units that fit one session (in roughly increasing order):
- `data/prefixes.rs` codegen + verification (1 session, foundational).
- A single hack file (1 session each — 58 of them, parallel-agent
  territory).
- `info.rs` + entry shell (1 session, low-stakes).
- `supports.rs` standalone (1–2 sessions, depends on data table).
- `transition.rs` standalone (1–2 sessions).
- `Prefixes::new` body (1 session, depends on data table).
- `processor.rs` (the big one — 2–3 sessions easily).

Pick one. Take it 0 → 100%. Move on.
