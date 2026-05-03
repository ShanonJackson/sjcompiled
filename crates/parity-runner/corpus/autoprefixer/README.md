# Autoprefixer parity corpus

Flat `*.css` fixtures consumed by `parity-runner --stage autoprefixer`. Each
file is a focused JS-vs-Rust byte-equality check: small enough that a
single divergence narrows quickly to one cause.

## Browserslist pin

Both engines must resolve to AFM's exact 14-entry browser list. The
parity-runner stage (when AGENT_4 Pass 2 lands and AGENT_6 wires it) will
pin via `BrowsersOptions::from = afm_fixture_dir()` on the Rust side and
`BROWSERSLIST_CONFIG = afm_fixture_dir/.browserslistrc` on the JS bridge
side. The AFM `.browserslistrc` lives at
`crates/browserslist-shim/tests/fixtures/afm/.browserslistrc` — SHA256
`08c8e1bf56ad773621c9b264971365f66f78a808d6d369a4ea9584a02da459cb`. See
HANDOVER.md §6.

## Fixture taxonomy

Numeric prefix bins inputs by purpose:

| Range  | Bucket                    | Stresses                                                 |
|--------|---------------------------|----------------------------------------------------------|
| 001-039 | Walk-targeted (Pass 2)   | One CSS surface per file: declaration / value / selector / at-rule / supports / transition / resolution. Fires once `Processor::add` walks land. |
| 040-049 | Helper-targeted (Pass 1) | `autoprefixer: ignore next` / `off` / `on`, `autoprefixer grid: autoplace` / `no-autoplace`, `@supports (grid auto)` override. Exercises AGENT_4 Pass 1 helpers. |
| 050-059 | Negative / no-op         | Inputs that produce no prefix activity under the AFM browserslist. Proves the negative path. |
| 060-069 | AFM real-shape           | Real AFM-React CSS shapes lifted from AGENT_5's `_phase_a_scratch/afm_synthetic_corpus/`. Multi-selector files that stress the integration surface as a whole. |

## Coverage map (HANDOVER §9 + AGENT_4 recommendation)

- Each `Browsers.prefixes()` value gets ≥1 fixture: `display: flex` / `display: grid` / `@keyframes` / `@supports` / `transition: transform` / `linear-gradient` / `:fullscreen` / `::placeholder` / `::file-selector-button`.
- AFM-in-scope hack subset (per AGENT_5's instrumentation): `user-select`, `text-decoration` (non-basic value), `text-decoration-skip-ink`, `intrinsic` (`fit-content` / `fill-available` / `stretch`), `cross-fade()`.
- A "no-op" fixture exercises an input where the AFM browser list produces no prefixing activity at all.
- A "mixed already-prefixed + unprefixed" fixture exercises the `isAlready` + `otherPrefixes` interaction inside a single rule.
- Helper-targeted comment fixtures exercise AGENT_4's Pass 1 helpers (`disabled`, `gridStatus`) immediately, even before Pass 2's walks land — useful as a thin Processor::new smoke test.

## Adding a fixture

1. Pick the next available numeric prefix in the right bucket.
2. Keep it ≤5 selectors. The point is fast diff localisation.
3. The filename's stem becomes the parity-runner label — make it
   descriptive (`011-order` not `011`).
4. After AGENT_4 Pass 2 lands, run
   `cargo run -p parity-runner -- --stage autoprefixer --corpus crates/parity-runner/corpus/autoprefixer`
   to verify byte-clean against the JS oracle.
