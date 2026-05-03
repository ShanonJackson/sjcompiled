# browserslist-shim — AFM port notes

> **Read this before touching `src/index.rs` or `src/parse.rs`.** It captures
> WHY the resolver looks the way it does — specifically why we have a
> hybrid AFM-fast-path / `oxc_browserslist` fallback architecture instead
> of either (a) full upstream port or (b) pure-oxc delegation.

---

## TL;DR

- The shim resolves browserslist queries via **two paths**, chosen automatically:
  1. **AFM fast path** — byte-correct against `caniuse-db@1.0.30001766`. Used when every atom in the query is one we recognise.
  2. **`oxc_browserslist` fallback** — drift-tolerant. Used when any atom is outside the AFM grammar.
- AFM's runtime call (autoprefixer → `browserslist(null, { path: jira/ })` → AFM `.browserslistrc`) ALWAYS goes down the fast path. That's the path the parity gate verifies.
- Phase 6 cssnano consumers (`postcss-normalize-unicode`, `postcss-colormin`, `postcss-minify-params`, `caniuse-api::clean_browsers_list`) ALWAYS go down the fallback. Their consumers reduce shim output to a drift-stable boolean (set intersections that never include current-versions chrome), so the oxc snapshot drift doesn't reach hash bytes.
- **Do not remove the fallback** without re-auditing every cssnano consumer. The user's explicit guidance was to keep this scaffolding around because AFM's `.browserslistrc` may change later and we'd want oxc-style drift-tolerance available again.

---

## Background — why the previous gate was open

`oxc_browserslist@3.0.2` (the workspace pin) bundles its own caniuse-lite snapshot. That snapshot is ~2 chrome releases newer than our pinned `caniuse-lite@1.0.30001766`. For "current versions" queries (`defaults`, `> 1%`, `last 2 versions`, `chrome >= 50`) oxc returns `chrome 145, chrome 146` where AFM's pinned JS oracle returns `chrome 143, chrome 144`. That cascaded into:

- `Browsers::new(query)` → drifted `selected` list
- `Prefixes::new(selected, data)` → drifted `add_table` / `remove_table`
- `processor.rs` walk → drifted prefix decisions
- `transform.ts` output → drifted hash → every consumer class name rotates

The gate at `crates/autoprefixer/tests/browserslist_parity.rs::browserslist_shim_matches_js_oracle_for_canonical_queries` was `#[ignore]`'d to document this. Closing it was the single biggest unblocker for `Prefixes::new` / `processor.rs` work.

Three closure options were on the table (see `crates/autoprefixer/MORNING.md` Options A/B/C):

- **(a)** Inject our snapshot into `oxc_browserslist` — multi-day, requires upstream PR or fork. Bundled `.bin.deflate` blobs make data injection painful. Rejected.
- **(b)** Re-port `browserslist@4.24.2`'s `index.js::resolve` against `caniuse-db` directly — cleanest deterministic path. Multi-day estimate originally; shrunk to one session after AFM's runtime instrumentation revealed the actual query surface.
- **(c)** Downgrade `oxc-browserslist` to a version (`2.3.0`) whose bundled snapshot has `chrome 144` — looks closer in the chrome slice but bundled caniuse-lite usage percentages still drift in the trailing decimals, breaking percentage-thresholded queries (`> 1%`, `defaults`). Rejected as false progress.

We picked **(b), descoped to AFM's actual query surface**.

---

## What AFM actually passes

Per `BROWSER_LIST_FROM_AFM.md` (runtime-instrumented from the actual `jira/` build):

- `@compiled/css@0.19.0` calls `autoprefixer()` with no args.
- Autoprefixer calls `browserslist(null, { path: cwd })`.
- cwd at build time is `/home/ubuntu/atlassian-frontend-monorepo/jira/`.
- That walks up to `jira/.browserslistrc` (SHA-256 `08c8e1bf56ad773621c9b264971365f66f78a808d6d369a4ea9584a02da459cb`).
- The `.browserslistrc` content (after `#`-comment stripping):

  ```
  last 2 Edge version
  last 2 Firefox version
  last 5 Chrome version
  last 2 Safari version
  last 2 iOS version
  last 2 ChromeAndroid version
  ```

- Resolved output (frozen oracle, 14 entries):

  ```
  and_chr 144
  chrome 144, 143, 142, 141, 140
  edge 144, 143
  firefox 147, 146
  ios_saf 26.2, 26.1
  safari 26.2, 26.1
  ```

The AFM `.browserslistrc` is byte-copied to `tests/fixtures/afm/.browserslistrc`; the SHA-256 is asserted by `tests/afm_parity.rs::afm_browserslistrc_fixture_sha256_matches`.

---

## AFM-fast-path grammar

Implemented in `src/parse.rs` as `try_parse_atom_afm(&str) -> Option<QueryAtom>`. Two variants:

| `QueryAtom` variant | Matches | Used for |
|---|---|---|
| `LastNBrowserVersions { n, browser }` | `last N <browser> version[s]?` (case-insensitive) | AFM's `.browserslistrc` (the only atom present) |
| `BrowserVersion { browser, version }` | `<browser> <version>` literal (e.g. `firefox 115`) | Output of the Firefox ESR rewrite (`Firefox ESR` → `firefox 115, firefox 128`) |

Browser names are canonicalised at parse time: lowercased + aliased per `browserslist@4.24.2` `aliases` map (`Edge → edge`, `iOS → ios_saf`, `ChromeAndroid → and_chr`, `FF → firefox`, etc).

`try_parse_all_afm(&[String]) -> Option<Vec<QueryAtom>>` is unanimous-or-none: if ANY atom is unrecognised, the whole query falls back to oxc. A partial mix would silently drift, which we do not allow.

---

## Resolver semantics (AFM fast path)

`src/index.rs::resolve_afm_atoms`:

1. For each atom, build a `Vec<String>` of `"<browser> <version>"` distributions:
   - `LastNBrowserVersions`: look up `caniuse_db::agents::agent(browser)`. Compute `released = agent.versions.iter().filter_map(Option::as_deref).filter(|v| release_date.get(v).flatten().is_some())`. Take last `N`. Format as `"{browser} {version}"`.
   - `BrowserVersion`: look up agent. Find a version that equals the requested version OR contains it as a range entry (`"18.5-18.7"` matches `"18.6"`). Format as `"{browser} {version}"`.
2. Concatenate all atoms' results.
3. Dedupe preserving first occurrence (matches JS `uniq` at `index.js:67`).
4. Sort: same browser → descending semver; different browser → ascending name lexicographic. Mirrors JS `index.js:431-444` exactly.

The `released` computation (filter Nones AND filter entries with no `release_date`) is what makes the fast path produce `chrome 144` as the latest instead of `chrome 147`. Caniuse-lite tracks future planned versions in `versions` (with `release_date: null` until they ship). Browserslist's `data.released` excludes them; we mirror that.

---

## Fallback semantics (oxc path)

`src/index.rs::resolve_with` last branch — joins all atoms with `, ` and calls `oxc_browserslist::resolve(&[joined], &Opts::default())`. Returns oxc's `Vec<String>` directly, or `Vec::new()` on parse error.

This is what every `> X%`, `<= 15`, `not all`, `not dead`, `last 2 versions` (no browser), `defaults`, etc. atom routes to. Phase 6 consumers using these:

- `cssnano-postcss-normalize-unicode` — `resolve("", true)` (defaults) and `resolve("ie <=11, edge <= 15", true)` (legacy bug query). Output reduces to `is_legacy: bool` from set intersection. Drift-stable.
- `cssnano-postcss-colormin` — `resolve(user_query, true)` (forwarded user opt). Bracketed by AFM consumer always passing the same query.
- `cssnano-postcss-minify-params` — same shape as colormin.
- `caniuse-api::clean_browsers_list` — `resolve(query.unwrap_or(""), false)`. Used by `caniuse_api::is_supported` which Phase 6 cssnano plugins consult.

If a future consumer's output *does* propagate oxc drift to hash bytes, that consumer needs an AFM-fast-path extension — not a workaround in the fallback.

---

## Firefox ESR override

`Firefox ESR` (and aliases `ff esr`, `fx esr`) is rewritten BEFORE the fast-path-vs-fallback decision into the literal pair `firefox 115, firefox 128` (per `browserslist@4.24.2` `select()` at `index.js:1024`). The literals then resolve via the fast path's `BrowserVersion` atom.

This override exists because both `oxc_browserslist@2.x` AND `oxc_browserslist@3.x` hardcode `firefox_esr → firefox 140` (different from BOTH `browserslist@4.24.2` AND `4.24.4`). Without the rewrite, `Firefox ESR` would silently return the wrong version through the fallback.

Two implementations exist:
- `expand_firefox_esr_atom(&str) -> Vec<String>` — per-atom, used internally.
- `rewrite_firefox_esr(&str) -> String` — string-level (comma-joined input → comma-joined output). Kept for backwards-compat with the existing `rewrite_firefox_esr_unit` test and any external string-based callers.

Both are `pub` so future code can use either shape. Don't delete either without checking call sites.

---

## Adding a new atom (when AFM's `.browserslistrc` evolves)

1. Confirm the atom appears in AFM's `jira/.browserslistrc` via runtime instrumentation (re-run the protocol in `BROWSER_LIST_FROM_AFM.md`).
2. If the atom is genuinely new:
   - Add a variant to `parse::QueryAtom`.
   - Add a regex + capture-binding to `try_parse_atom_afm`.
   - Add a resolver branch to `index::resolve_afm_atoms`.
   - Add a unit test in `index.rs::tests::afm_fast_path_<atom_name>`.
   - Update the AFM fixture if the new atom is now in `.browserslistrc`. Re-compute SHA-256 and update the assertion in `tests/afm_parity.rs`.
   - Add a sentence to the "AFM-fast-path grammar" table above.
3. If the atom is supported by the fallback (most cases), do nothing — it'll silently route through oxc with documented drift tolerance.

**Do NOT widen the fast path opportunistically.** Each new variant doubles the surface area we're pinning to caniuse-db semantics. Only port what AFM actually consumes.

---

## What NOT to remove

Per the user's explicit guidance during landing this work:

> "I wouldn't remove any like browserlists stuff/hardcoded hacky workarounds for this EXACT browserlist TOO Much because later AFM's browser list may change but for now this is our focus"

Translation:

- **Keep `oxc_browserslist` in `Cargo.toml`.** It's the fallback. Remove it and every `> X%` / `<= 15` / `not all` / defaults caller breaks.
- **Keep both `expand_firefox_esr_atom` AND `rewrite_firefox_esr`.** The string-form is used by the existing test contract and may be called by external consumers we haven't audited.
- **Keep `node::default_query` returning the full `> 0.5%, last 2 versions, Firefox ESR, not dead` string** even though no AFM caller reaches it. The defaults-fallback path is reachable via cssnano consumers and exercised by `index::tests::defaults_resolve`.
- **Keep `node::find_config_file` walking ancestors.** AFM's call locates `.browserslistrc` in `jira/` directly, but other callers may be deeper in the tree.
- **Keep the `parse_package`/`pick_env` machinery.** AFM doesn't use `package.json#browserslist` or `[production]`/`[development]` sections, but the JS shim supports them and a future AFM repo migration might.

---

## Test surface

After this work:

| Test | Path | Notes |
|---|---|---|
| `parse::tests::*` | `src/parse.rs` | Grammar coverage for AFM atoms |
| `index::tests::afm_fast_path_*` | `src/index.rs` | Per-atom byte-clean checks against caniuse-db |
| `index::tests::firefox_esr_*` | `src/index.rs` | Firefox ESR rewrite + 2-version contract |
| `index::tests::defaults_resolve` | `src/index.rs` | Fallback path smoke (oxc returns >0 entries) |
| `index::tests::explicit_query_wins` | `src/index.rs` | Fallback path forwards `<=` queries to oxc |
| `index::tests::rewrite_firefox_esr_unit` | `src/index.rs` | String-form rewrite unit |
| `node::tests::*` | `src/node.rs` | `.browserslistrc` parsing / `package.json` parsing / config walk |
| `tests/afm_parity.rs::afm_browserslistrc_fixture_sha256_matches` | integration | Fixture file matches AFM-pinned hash |
| `tests/afm_parity.rs::afm_browserslistrc_resolves_to_frozen_oracle` | integration | End-to-end: shim against frozen 14-entry oracle |
| `crates/autoprefixer/tests/browserslist_parity.rs::browserslist_shim_matches_js_oracle_for_afm_browserslistrc` | autoprefixer test | End-to-end: bun (JS oracle) ↔ shim agreement on AFM `.browserslistrc` |
| `crates/autoprefixer/tests/browserslist_parity.rs::browserslist_shim_firefox_esr_matches_js_oracle` | autoprefixer test | bun ↔ shim agreement on `Firefox ESR` shim path |

Floor counts after closure:
- `cargo test -p browserslist-shim`: **29 passing**, 0 ignored (was 15 passing pre-port).
- `cargo test -p autoprefixer`: **60 passing**, 0 ignored (was 58 passing + 1 ignored — the ignored canonical-queries omnibus is gone, replaced by the AFM-fixture variant that runs always).

---

## What this work does NOT close

- **`Prefixes::new` body** — still `unimplemented!()` in `crates/autoprefixer/src/prefixes.rs`. This work was the pre-condition; that port is the next session's unit. See `crates/autoprefixer/HANDOVER.md` §1 + §12.
- **`processor.rs` main walk** — depends on `Prefixes::new`. ~720 LOC, multi-session.
- **Generic `Prefixes::new` byte-test against arbitrary queries** — only the AFM-shaped queries are byte-clean today. If `Prefixes::new` is ever called with a query that hits the fallback (which AFM never does, but a stray test might), output drifts.
