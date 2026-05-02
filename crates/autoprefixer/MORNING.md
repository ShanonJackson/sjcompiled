# MORNING — autoprefixer pickup prompt

You are picking up the `autoprefixer` Rust port from a previous agent.
Read this file end-to-end before doing anything else. It assumes you
know nothing about this project.

---

## What this project is

The user is porting `packages/css/src/transform.ts` — a CSS atomic
stylesheet generator — from JavaScript to Rust. The output of
`transform.ts` is fed into a class-name hash, and **a single byte of
divergence between JS and Rust output renames every CSS class in
production for ~10,000–20,000 consumer call sites** in a 60-90 GB
monorepo. The contract is byte-for-byte identical output, forever.

`transform.ts` calls 17+ postcss plugins. Each plugin is being ported
to Rust as a 1:1 mirror of a specific pinned npm version. **Bugs in
the upstream JS at the pinned version are bugs you replicate.** No
"improvements," no "fixes," no version bumps. Bytes are the contract.

Read `crates/PARITY_VERSIONS.md` for the version pins and the full
"Cardinal Rules" — that's the contract you're working under.

You have been assigned to the `autoprefixer@10.4.14` port. It's the
single largest plugin (~8k LOC of JS, estimated 8+ weeks for one
engineer total). Multiple agents have been chipping away at it over
several sessions. You are picking up the foundation-agent role.

---

## Read these in this order

1. **`crates/PARITY_VERSIONS.md`** — the contract. ~5 min.
2. **`crates/PLUGIN_IMPLEMENTATION_GUIDE.md`** — the framework you're
   working in (postcss-core's API, the `walk_*_mut_with_parent`
   family, `Node.attrs`, `clone_without`). ~10 min.
3. **`crates/STATUS.md`** — overall project state. Search for
   "Phase 7" and "Phase 7 split contract" — both relevant. ~5 min.
4. **`crates/autoprefixer/HANDOVER.md`** — exhaustive autoprefixer
   handover, includes maintenance guides + JS quirks list. **THE
   most important doc for your work.** ~15 min.
5. **`crates/autoprefixer/src/hacks/HACKS_PORT.md`** — 58-row
   per-hack table with parent classes + LOC. You will NOT port hacks
   (that's the parallel hacks agent's job) but skim it so you know
   what's in scope vs out. ~3 min.
6. This file (you're reading it). ~3 min.
7. **`crates/STATUS.md` "Path-shift gotcha"** section + **HANDOVER.md
   §11** (subtle JS quirks). ~5 min — these will save you hours.

Total: ~45 min of reading before you write a line of code.

---

## Where things stand right now (last session checkpoint)

`cargo test -p autoprefixer` → **57 passing** (53 unit + 4 parity).
This is the floor. Your work must keep it there or grow it.

**Done (full bodies, real tests):**
- `utils.rs`, `vendor.rs`, `brackets.rs`, `old_value.rs`,
  `old_selector.rs`
- `prefixer.rs` — base trait, `parent_prefix` walks via
  `walk_up_with`, `Node.attrs` cache, `clone_without` strip
- `browsers.rs` — caniuse-db agents + browserslist-shim wrap
- `at_rule.rs`, `value.rs`, `selector.rs`, `declaration.rs`,
  `resolution.rs` — full base classes
- `prefixes.rs` — `HackRegistry` skeleton with `register_hacks(reg)`
  append-only block; `Prefixes` orchestrator method bodies are
  `unimplemented!()`
- `data/prefixes.rs` — 183 entries, codegen via `build.rs` + `bun`,
  4 parity gates (canonical-JSON byte-equal, entry count, key order,
  caniuse-lite version pin) all green
- `resolution.rs::prefix_query` — `f.simplify(None)` call landed
  after fraction-js agent surfaced it as latent byte-divergence in
  `-o-` branch. Pinned by `prefix_query_o_dpcm_uses_simplify` test.

**Stubbed (`unimplemented!()`, panics on call):**
- `supports.rs` (302 LOC) — `@supports` query rewriting
- `transition.rs` (329 LOC) — `transition` shorthand handling
- `prefixes.rs::Prefixes::{new, cleaner, select, group}` — orchestrator
- `processor.rs` (718 LOC) — main walk; **the engine**
- `info.rs`, `autoprefixer.rs` — entry layer
- All 58 hack files — `src/hacks/*.rs` are 7-line stubs

**Not started:**
- Browserslist-shim parity gate against JS oracle (HANDOVER §6)
- `Stage::Autoprefixer` parity-runner stage + corpus + diff harness
- Wire-up into `crates/css/src/transform.rs`
- NAPI bridge for `transformCss` (Phase 8b)

**Important:** the autoprefixer port currently cannot prefix a single
line of CSS end-to-end. Calling `process()` on a stylesheet panics on
the first hit into `Prefixes::new` because that method is
`unimplemented!()`. The framework is in place; the engine isn't.

---

## Your unit for this session

**Pick ONE of the following.** Take it 0 → 100% byte-clean. Stop.

The cardinal rule (`STATUS.md`): a session takes a unit 0 → 100%
byte-clean. Half-done ports become silent byte-drift hazards across
agent handoffs. **Pick small enough to finish.**

Recommended order (each is ~one session):

### Option A — `Prefixes::new` body (RECOMMENDED)

Highest-leverage. Unblocks `processor.rs` (which can't be written
without `Prefixes` instantiable). Source is `prefixes.js` constructor
+ helper methods (~150 LOC of constructor logic out of the 428-LOC
file).

**Pre-condition:** the browserslist-shim parity gate. You need to know
that `Browsers::new(["defaults"])` produces a `selected` list that
matches JS `browserslist("defaults")` byte-for-byte. If it doesn't,
`Prefixes::new`'s output is silently wrong. **Check this BEFORE
porting `Prefixes::new`** — if the gate's open, close it first (write
JS-vs-Rust diff test in `crates/browserslist-shim/tests/` or
`crates/autoprefixer/tests/`). HANDOVER §6 has the rationale.

Fits in one session if browserslist parity is already green. ~2x
session length if you have to close it first.

### Option B — `supports.rs` (302 LOC)

`@supports (display: flex)` query rewriting. Independent base class —
doesn't depend on `Prefixes::new`. Self-contained, byte-testable in
isolation. Good fallback if `Prefixes::new` looks too big.

### Option C — `transition.rs` (329 LOC)

`transition: transform 0.3s` — when the value contains a property name
(`transform`), the transition needs prefix-matched siblings on the
declaration. Independent base class. Slightly trickier than `supports`
because it interacts with the `Declaration` class for sibling
discovery. Doable but a stretch for one session.

### Option D — close the browserslist-shim parity gate (HANDOVER §6)

If you don't trust your time-box for any of A/B/C, this is the
cleanest pre-condition for whoever picks up `Prefixes::new` next. Write
canonical browserslist queries (`"defaults"`, `"> 1%"`, `"chrome >=
50"`, `"last 2 versions"`, `"Firefox ESR"`, `"not dead"`) into a test
that compares Rust `browserslist_shim::resolve` output against JS
`browserslist(query)` (via `bun -e`) element-by-element. Land it as
a test in `crates/browserslist-shim/tests/` (or
`crates/autoprefixer/tests/` if browserslist-shim isn't yours to
modify — ask first).

**My recommendation: Option A if browserslist parity is already
green, Option D otherwise. DO NOT pick Option B or C without
confirming that the user wants this rather than the engine path.**

---

## What you will NOT do

These are explicit no-go zones for this session:

1. **Do not port any hacks.** Everything under `src/hacks/*.rs` is the
   parallel hacks agent's territory. Even if a hack looks "trivial,"
   crossing the boundary creates merge conflicts.
2. **Do not edit `parity-runner/src/stages.rs`, `parity-runner/src/main.rs`,
   or `packages/css/scripts/parity-bridge.mjs` without asking.** Those
   are workspace-shared. Wiring a `Stage::Autoprefixer` stage is
   premature anyway — gates on `processor.rs` existing, which is
   multiple sessions out.
3. **Do not edit `crates/css/src/transform.rs`.** That's where the
   final wire-up happens. Way out of scope.
4. **Do not bump any pinned version** (`autoprefixer`, `caniuse-lite`,
   `browserslist`, `postcss`, anything in `PARITY_VERSIONS.md`). The
   default answer to "should we bump this?" is NO.
5. **Do not "fix" upstream bugs.** If you find a bug in
   `autoprefixer@10.4.14`'s JS that you'd rather not replicate — replicate
   it anyway. File the bug, link upstream, move on.
6. **Do not write your own tree walk.** Use
   `postcss_core::walk_*_mut_with_parent` family. They handle the
   cursor-shift bug for you (see HANDOVER §3).
7. **Do not `format!("{}", f64)` for any output bytes.** Use
   `postcss_core::js_number_to_string`. Rust's f64 Display diverges
   from V8 at the edges.
8. **Do not use `HashMap` anywhere on the hashing path.** `IndexMap`
   only. Iteration order reaches output bytes.
9. **Do not skip the verification gates.** Before claiming a unit is
   done: `cargo test -p autoprefixer` (57+ passing), `cargo build -p
   autoprefixer` (clean), and if you wired anything new, the relevant
   parity test must be byte-clean against the JS oracle.

---

## The gotchas that will bite you (one paragraph each)

**Path-shift bug.** Every `insert_before_at_path(root, path, ...)`
shifts the original's index up by 1. If you're calling it in a loop,
the path becomes stale. The fix pattern is in `at_rule.rs::process`,
`declaration.rs::process`, `selector.rs::add` — bump
`current_path.last_mut() += 1` after every successful insert. Tests
that exercise only ONE prefix per node are silent on this bug; tests
must exercise ≥2 prefixes to catch it. HANDOVER §3 has details.

**`Node.attrs` keys must be namespaced and listed in
`prefixer::CLONE_STRIP_KEYS`.** If you add a new memo, add it to that
list — `prefixer::clone_node` calls `Node::clone_without(CLONE_STRIP_KEYS)`
and a clone with stale memos silently breaks parity. The recursive
strip is already handled by `Node::clone_without`. Existing keys:
`_autoprefixerPrefix`, `_autoprefixerValues`, `_autoprefixerCascade`,
`_autoprefixerMax`, `_autoprefixerPrefixeds`, `proxyCache`.

**`bun install` is a build pre-condition.** `data/prefixes.rs`'s
`build.rs` requires `node_modules/caniuse-lite` to exist at exactly
version `1.0.30001690`. If `bun install` hasn't run, `cargo build`
fails with a clear directive. If a previous bun.lock floated past the
pin, `caniuse_lite_pin_matches_parity_versions` test fails. The fix
is in `package.json` `overrides` + `devDependencies`. HANDOVER §2
has the maintenance guide.

**`decl.raws.before` is `Option<String>`.** JS treats it as `string`
default `''`. Use `.as_deref().map(...).unwrap_or(false)` for boolean
predicates and `.clone().unwrap_or_default()` for string ops. Direct
field access does not compile.

**Don't conflate `simplify` with GCD reduction.** JS fraction-js
`simplify(eps=0.001)` does continued-fraction approximation, NOT just
gcd. The previous agent omitted it in `resolution.rs`, which silently
produced wrong `-o-` resolution bytes. Fix landed; the
`prefix_query_o_dpcm_uses_simplify` test pins it. **If you write any
code that calls fraction-js, check whether the JS upstream calls
`simplify()` somewhere — and replicate the call.**

---

## Workspace state at handoff

- `cargo test -p autoprefixer` → 57/57 passing.
- `cargo build -p autoprefixer` → clean.
- `cargo check --workspace` → clean (verified end of last session).
- Other agents in parallel: `postcss-core` (done), `postcss-nested`
  (done), `postcss-normalize-positions/timing-functions/url` (all
  done), `fraction-js` (done — including the `simplify` add I now
  consume). Workspace test totals last seen ~502+ but stale; run
  `cargo test --workspace --no-fail-fast` if you want a fresh number
  before starting.
- `caniuse-lite` pinned at `1.0.30001690` workspace-wide via
  `package.json` overrides + devDep. Don't touch.
- TaskList: I left task #8 split (declaration.js DONE, supports.js
  + transition.js still pending). Task #10 is "Port processor.js +
  info.js + autoprefixer.js (top-level entry)" — that's still
  pending. Task #3 ("Port data/prefixes.js → data/prefixes.rs") is
  technically still marked pending in the list but reality is DONE
  via codegen. Mark it completed when you start your session if
  you're cleaning up.

---

## Sign-off checklist (before you stop)

Before you mark your session complete, run these in order:

1. `RUSTFLAGS="" cargo test -p autoprefixer` — must show ≥57 passing.
2. `RUSTFLAGS="" cargo build -p autoprefixer` — must be clean.
3. `RUSTFLAGS="" cargo check --workspace` — must be clean (catches
   accidental cross-crate breakage).
4. If you added a new test for byte-equality vs JS oracle — run that
   test specifically and read its output. Don't trust an aggregate
   "57/57" that hides a non-running parity test.
5. Update `crates/STATUS.md` Phase 7 row + test count + the
   "Foundation agent's responsibilities" checklist (whichever item
   you completed flips to ✅).
6. Update `crates/autoprefixer/HANDOVER.md` §1 (test count floor) +
   the "What's real" / "What's stubbed" lists.
7. Update `crates/autoprefixer/HANDOVER.md` §11 if you discovered a
   new JS quirk.
8. Write a `MORNING.md`-style handoff for the NEXT agent (overwrite
   this file or append a "today's session" section). Include what you
   completed, what you observed about other agents' work, and what
   the next-highest-leverage unit is.
9. Mark your task complete in TaskList. Don't leave it `in_progress`.

If any of 1-3 fail and you can't fix them in 10 minutes, **roll
back** to the last green state with `git restore` on your changes.
Better an honest "I tried this approach and rolled back" than a half-
landing that leaves the floor below 57.

---

## If you have to push back on scope

Per HANDOVER §14: the full Phase 7 port is 8+ weeks. We're maybe 3-4
days in. **It is normal and expected for a session to NOT finish a
unit if the unit was misjudged.** When that happens:

- Don't half-land. Roll back.
- Pick a smaller unit and finish that one cleanly.
- Document in MORNING.md what you tried, why the unit was bigger
  than expected, and what the new estimate is.

The user knows this is multi-session work. They prioritize honest
incremental progress over false "we're nearly done" claims. Read
HANDOVER §14 again if you feel pressure to push past your time-box.

---

## When you're stuck

The vendored upstream JS lives at:
```
crates/_vendor/autoprefixer-10.4.14/package/lib/<file>.js
crates/_vendor/postcss-8.4.31/package/lib/<file>.js
```

Read those. NOT the latest GitHub source. NOT Stack Overflow. Pinned
versions only.

For postcss-internal questions (how does `raws.between` flow?), read
`crates/postcss-core/src/{parser,stringifier}.rs` first.

For autoprefixer-internal questions, the previous agent left a
HANDOVER.md that's deliberately exhaustive. Search it before asking.

---

## Final note from the previous agent

The work that remains is bigger than the work that's done. The
framework + base classes + data table are in; the engine isn't, and
neither is the byte-emitting hack logic that drives 40% of real-world
autoprefixer output. Be realistic about scope. Take ONE unit. Finish
it. Hand off cleanly.

Good luck.
