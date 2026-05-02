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


## `compiled-css::sort_at_rules::locale_compare_en` is byte cmp, not UCA

**Where:** `crates/compiled-css/src/plugins/at_rules/sort_at_rules.rs:69-71`.

**What:** JS `sort-at-rules.ts:72` (and the stage-4 tiebreaker at line
107) call `String.prototype.localeCompare(other, 'en')`, which follows
the Unicode Collation Algorithm (`'en'` locale folds diacritics, treats
`ä` ≈ `a`, etc.). The Rust port falls back to byte-wise `str::cmp`.

**Why we don't fix it:** binding a UCA collator costs ~10 MB (icu_collator
+ CLDR data) or a hand-port of the entire `'en'` collation table. The
project will eventually compile to WASI/WASM (CLAUDE.md "WASI/WASM
Compilation" section explicitly bans 10 MB deps), and the affected code
path is the stage-4 tiebreaker between two at-rules whose names are
equal AND whose breakpoint sequences are equal AND only their original
`query` strings differ AND those queries contain non-ASCII tokens. Stage 1
(at-rule name) is safe — CSS at-rule names are ASCII per spec including
vendor prefixes.

**Inputs that would expose it:** project source naming a `@layer`,
`@container`, or `@scope` with non-ASCII tokens that interact with the
stage-4 tiebreaker — e.g. `@layer ärea` and `@layer azul`. JS sorts
`ärea < azul` (UCA folds `ä→a`); Rust sorts `azul < ärea` (UTF-8
`0xC3 > 0x7A`).

**Demonstrated divergence:** the parity-runner corpus entry
`sort-atomic-style-sheet/18_non_ascii_at_rule_queries.css` (since
removed) failed at byte 0 with this exact pair.

**First flagged:** `crates/_vendor/COMPILED_CSS_LOCAL_PLUGINS_AFM_REAUDIT.md` (2026-05-03).


## `compiled-css::discard_empty_rules::is_js_whitespace` over-strips vs ECMA-262

**Where:** `crates/compiled-css/src/plugins/discard_empty_rules.rs:119-125`.

**What:** Rust uses `char::is_whitespace()` (Unicode `White_Space`
property) plus an explicit U+FEFF carve-in. JS `String.prototype.trim`
strips the ECMA-262 Table 33 set: TAB, LF, VT, FF, CR, SP, NBSP, ZWNBSP,
LS (U+2028), PS (U+2029), and any character in category Zs. Rust's
predicate is a strict superset: it also strips U+0085 NEL and U+1680
OGHAM (both `White_Space` per Unicode but neither in Table 33).

**Why we don't fix it:** the only inputs that trigger drift are CSS
declaration values consisting *entirely* of NEL or OGHAM characters.
NEL is mainframe / EBCDIC-origin and effectively never appears in
modern CSS. OGHAM is an ancient Irish script used in CSS values…
basically never. The realistic offenders (NBSP, regular spaces,
CR/LF/TAB) are handled identically by both predicates.

**Inputs that would expose it:** a declaration whose value, after parser
trimming, is exclusively U+0085 or U+1680 — JS `.trim() === ''` returns
false (decl kept); Rust returns true (decl dropped).

**Demonstrated divergence:** the parity-runner corpus entry
`discard-empty-rules/17_nel_value.css` (since removed) failed: Rust
produced `a { color: red; }` while JS preserved `padding: \u{85};`.

**First flagged:** `crates/_vendor/COMPILED_CSS_LOCAL_PLUGINS_AFM_REAUDIT.md` (2026-05-03).
