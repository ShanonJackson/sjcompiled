# Possible drift causes

Living list of known or suspected sources of byte-level drift between
the JS oracle and the Rust port. Each entry should describe what the
divergence is, where it lives, why it hasn't been fixed yet, and what
inputs would actually expose it.

---

## `oxc-browserslist@3.0.2` bundles its own `caniuse-lite` snapshot

**Where:** `crates/browserslist-shim/src/index.rs` delegates query
resolution to `oxc_browserslist::resolve()`. The `oxc-browserslist@3.0.2`
crate bundles its **own** `caniuse-lite` snapshot as compressed binary
blobs at `src/generated/caniuse_*.bin.deflate` — independent of our
`caniuse-db` snapshot.

**The split:** browser-version sets returned by queries like `> 0.5%`
come from oxc's bundled data, while feature support lookups downstream
(in `crates/autoprefixer/src/browsers.rs`, in `crates/caniuse-api`)
come from our `crates/caniuse-db`.

**Failure mode:** if oxc's bundled `caniuse-lite` version differs from
ours (`1.0.30001766`), the resolved browser-version list may include
versions our `caniuse-db` doesn't know about (silent miss in feature
lookup) or exclude versions we do (silent under-prefix). Either way the
hash drifts vs. the JS oracle.

**Why not fixed:** the gap exists at *any* `caniuse-lite` pin, not
specifically `1.0.30001766`. The 20-stage parity corpus is currently
byte-clean, which suggests AFM input does not exercise the divergence
in practice — but a worst-case AFM input (e.g. a `.browserslistrc`
querying only the very latest browser versions) could still trip it.
Closing the gap requires either:

- a custom `Distrib` source that backs `oxc_browserslist` with our
  `caniuse-db` data, or
- a new shim that re-implements the query grammar entirely against our
  data tables (heavier but cleanest).

A lighter intermediate: a parity gate that asserts oxc's bundled
chrome-version cap equals our snapshot's. That would at least make the
drift visible at CI time.

**Inputs that would expose it:** any `.browserslistrc` or query string
that resolves to a browser-version newer than oxc's bundled cap but
older than `caniuse-db`'s cap (or vice versa). Specifically anything
of the form `last 1 chrome version`, `last 2 versions`, or
`> 0.01% in alt-AS` against late-2025 browser releases.

**First flagged:** `crates/_vendor/CANIUSE_LITE_1.0.30001690_TO_1.0.30001766_AUDIT.md` (2026-05-02).
