# AGENT_1 — Done

## Test count delta

`cargo test -p autoprefixer`:
- **Before:** 60 passing (53 unit + 4 data parity + 3 browserslist parity), 0 failing, 0 ignored.
- **After:** 73 passing (66 unit + 4 data parity + 3 browserslist parity), 0 failing, 0 ignored.
- **+13 unit tests** — 7 in `prefixes::tests`, 1 in `declaration::tests::restore_before_replaces_tail_line_with_shortest_in_group`, 3 in `autoprefixer::tests`, 2 in `prefixes::tests` (sort + unprefixed_prop).

All three sign-off gates green:
```
RUSTFLAGS="" cargo test -p autoprefixer        # 73 passing, 0 failing, 0 ignored
RUSTFLAGS="" cargo build -p autoprefixer       # clean
RUSTFLAGS="" cargo check --workspace           # clean (warnings are all pre-existing in other crates)
```

## Files changed

| File | Change |
|---|---|
| `crates/autoprefixer/src/prefixes.rs` | Replaced the 4 `unimplemented!()` method bodies with real ports of `prefixes.js`'s `constructor` / `cleaner` / `select` / `sort` / `group` / `unprefixed` / `normalize` / `prefixed` / `values` (last is a stub). Added `PrefixesOptions`, `Selected`, `GroupView<'a>` types. Added `cleaner_cache: OnceCell<Box<Prefixes>>` for the JS `cleanerCache` semantics. Added 7 unit tests (sort ordering, unprefixed_prop quirks, new/cleaner/group behaviour). |
| `crates/autoprefixer/src/declaration.rs` | Filled in `restore_before` body: walks `Prefixes::group(decl).up(...)`, finds shortest tail-line, writes back. Signature changed from `restore_before(&self, &mut Node)` (no-op) to `restore_before(&self, &Prefixes, &mut Node, &[usize])`. Added 1 unit test. **`DeclarationBase::process` does NOT yet call `restore_before` — that wiring is AGENT_4's responsibility (see "Anything I didn't finish" below).** |
| `crates/autoprefixer/src/autoprefixer.rs` | Replaced bare stub with `AutoprefixerOptions` struct + `build_prefixes(reqs, options) -> Result<Prefixes, AutoprefixerError>` + `build_prefixes_default(from)` convenience. Added 3 unit tests against AFM fixture path. **Does NOT wire the postcss-plugin shape (`prepare(result)` / `OnceExit(root)` hooks)** — that depends on `Processor::add` / `Processor::remove` which are AGENT_4's stubs. |
| `crates/autoprefixer/src/info.rs` | Updated module-level doc comment to clarify the bare-shell rationale (per HANDOVER §10 — info.js's diagnostic function is not on the hashing path). No code change. |

## JS quirks discovered (controller agent: fold into HANDOVER §11)

1. **`Prefixes::select` and 3-part raw browser strings.** `data.browsers` entries can be 3-part like `"chrome 100 2009"` — the third part is a "note" (typically `2009` or `2012` for old flexbox specs). JS `select()` does `let [name, version] = i.split(' ')` which destructures-and-drops the third part. Our Rust `Browsers::prefix(browser)` uses `splitn(2, ' ')` which carries the third part into the `version` variable — usually harmless because no `prefix_exceptions` key contains a space, but a latent divergence. **Worked around in `Prefixes::select` by explicitly trimming to 2 parts before passing to `Browsers::prefix`.** A future Browsers refactor that switches to full split + take-first-2 would let us drop the workaround; not in scope today.

2. **`unprefixed(prop)` flex-direction post-rewrite is independent of any hack.** JS:
   ```js
   unprefixed(prop) {
     let value = this.normalize(vendor.unprefixed(prop))
     if (value === 'flex-direction') value = 'flex-flow'
     return value
   }
   ```
   The post-normalize `if (value === 'flex-direction')` is a fallback for the case where the registered `flex-direction` hack does NOT override `normalize`. With AGENT_5's hacks landing, `flex-direction.js` likely overrides normalize to return `'flex-flow'` directly — but the autoprefixer-level `if` is defensive belt-and-braces. Ported verbatim for byte equality.

3. **`restoreBefore` keeps the STRING, not the length.** JS `let min = lines[lines.length - 1]` stores the actual leading-whitespace string, then compares lengths. Final write reuses the string — not `' '.repeat(min_length)`. Non-space leading chars (tabs, mixed indentation) are preserved verbatim. Mirrored in Rust.

4. **`Prefixes::cleaner` returns `&self` as the early-out path** when `browsers.selected` is empty. JS:
   ```js
   if (this.browsers.selected.length) {
     this.cleanerCache = new Prefixes(this.data, empty, this.options)
   } else {
     return this  // ← early return; cleanerCache stays unset
   }
   ```
   The Rust `OnceCell<Box<Prefixes>>` is left uninitialised on the empty-browsers path — multiple `cleaner()` calls on an empty-browser Prefixes always return `&self` directly. Pinned by `cleaner_returns_self_when_no_browsers_selected`.

5. **`group(decl).up/down` checker uses `Browsers::with_prefix` to detect prefix-only siblings.** JS `Browsers.withPrefix(other.value)` checks if a sibling's *value* contains a vendor prefix — used to break the run when the unprefixed sibling is a non-prefixed-value-of-prefix-property. Mirrored via the static `Browsers::with_prefix(value: &str)` already in `browsers.rs`. No new helper needed.

## Base-class methods I wished existed but didn't add

None. Every method I needed was either already on the base class (`Browsers::is_selected`, `Browsers::prefix`, `Browsers::with_prefix`, `vendor::unprefixed`, `utils::uniq`, `utils::remove_note`) or could be implemented inside `prefixes.rs` itself. AGENT_4 (processor) and AGENT_5 (hacks) should not need to extend any base trait on my behalf.

## Things I didn't finish that were in scope, and why

1. **`DeclarationBase::process` does NOT yet call `restore_before`.** JS:
   ```js
   process(decl, result) {
     if (!this.needCascade(decl)) { super.process(decl, result); return }
     let prefixes = super.process(decl, result)
     if (!prefixes || !prefixes.length) return
     this.restoreBefore(decl)  // ← not called from Rust process()
     decl.raws.before = this.calcBefore(prefixes, decl)
   }
   ```
   Wiring this requires `DeclarationBase::process` to receive `&Prefixes` (or to know its `all_id` lookup against the Processor's registry). The current process signature is `(&self, &mut Node, &[usize])`. Changing it would either:
   - Force `prefixes: &Prefixes` as a new arg, breaking declaration.rs's existing 9 unit tests, OR
   - Force a `prefixes: Option<&Prefixes>` arg, leaving the same JS-divergence latent.

   AGENT_4's `processor.rs` work is the natural place to thread `&Prefixes` into the per-decl process call (the Processor walk holds the Prefixes reference anyway). Leaving the wiring there. **The body of `restore_before` is correct and unit-tested standalone**, ready to be called once AGENT_4 lands the wiring.

2. **`Prefixes::values(type, prop)` is a stub returning `Vec::new()`.** Its real body (merge `add['*'].values` with `add[prop].values`) requires `preprocess()` to have run, which AGENT_5 owns. Stubbed body documents the dependency.

3. **`Prefixes::preprocess()` is NOT ported.** Per the brief, this depends on the hack registry (AGENT_5) and `Selector::load` / `Value::load` / `Declaration::load` factory methods that don't exist yet. The current `add_table` / `remove_table` shape (`IndexMap<String, Vec<String>>`) holds the post-`select()` data; AGENT_4 / AGENT_5 will add a `preprocess()` step that consumes them.

4. **Postcss-plugin shape (`OnceExit` hook).** `autoprefixer.js`'s real return value is a postcss plugin object with `prepare(result)` returning `{ OnceExit(root) }`. Ported only the constructor side (`build_prefixes`). The plugin-shape wiring belongs in `processor.rs` because `OnceExit` calls `prefixes.processor.{add,remove}(root, result)`.

## `BrowsersOptions::from` test pattern

Per HANDOVER §6: every test consuming `Browsers::new(...)` MUST set `BrowsersOptions::from` explicitly. My tests use this helper:

```rust
fn afm_fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("browserslist-shim")
        .join("tests").join("fixtures").join("afm")
}

fn afm_browsers() -> Browsers {
    let opts = BrowsersOptions {
        from: Some(afm_fixture_dir().to_string_lossy().into_owned()),
    };
    Browsers::new(Vec::new(), opts, BrowserslistOpts::default())
}
```

Copy this pattern verbatim. The empty-query path is intentional — it exercises the AFM `.browserslistrc` walk at `crates/browserslist-shim/tests/fixtures/afm/`. For tests that need an empty `selected` list (the `cleaner` early-return path), construct `Browsers` directly:

```rust
let empty = Browsers {
    selected: Vec::new(),
    options: BrowsersOptions::default(),
    browserslist_opts: BrowserslistOpts::default(),
};
```

Both patterns are exercised under `prefixes::tests::*`.

## AGENT_4 — you are unblocked

`Prefixes::new(browsers, options) -> Prefixes` exists, is byte-clean for AFM-shaped queries, and `processor.rs` can now consume it. The shape AGENT_4 will read:

- `prefixes.add_table: IndexMap<String, Vec<String>>` — name → prefixes to ADD.
- `prefixes.remove_table: IndexMap<String, Vec<String>>` — name → prefixes to REMOVE.
- `prefixes.cleaner() -> &Prefixes` — cached empty-browsers Prefixes for the remove pass.
- `prefixes.group(root, decl_path) -> Option<GroupView<'_>>` — for `restoreBefore` and `isAlready`.
- `prefixes.unprefixed_prop(prop) -> String` — `flex-direction → flex-flow` quirk applied.
- `prefixes.options: PrefixesOptions` — `cascade` / `add` / `remove` / `supports` / `grid` / `flexbox` toggles.

What AGENT_4 still needs to build:

- A `preprocess()` step that consumes `add_table` / `remove_table` and dispatches each `name` to a Selector / Value / Declaration / AtRule / Resolution subclass (via the eventual hack registry from AGENT_5). For now, the registry is empty — every dispatch falls back to the base class.
- The main walk in `processor.rs::process(prefixes, root)` that runs:
  1. `cleaner().process_remove(root)` — strip stale prefixes.
  2. `process_add(root)` — add needed prefixes.
- Wire `DeclarationBase::process` to call `restore_before(prefixes, root, path)` after the cascade-needed branch fires. This is the one-line tweak that closes the upstream `__tests__/cascade.test.js` byte gate.
- The postcss-plugin shape (`OnceExit(root)`) wrapping the walk. Live up at `autoprefixer.rs` once `processor.rs::process` exists, calling into `build_prefixes` then `process(...)`.

AGENT_5 (hacks) is independent of my work — they can register against the existing `HackRegistry` skeleton in this file (the `register_hacks` BEGIN/END block).

## Floor that must NOT regress

Final test count: **73 passing, 0 failing, 0 ignored**. Anyone who lands work after me must keep this number at ≥73 or grow it. Run before EVERY commit:

```bash
cd crates
RUSTFLAGS="" cargo test -p autoprefixer
RUSTFLAGS="" cargo build -p autoprefixer
RUSTFLAGS="" cargo check --workspace
```
