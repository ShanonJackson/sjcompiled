# `utils/css_builders.rs` — `@compiled/css` re-export wiring

> **Status:** RESOLVED 2026-05-04. Both helpers are now ported and
> publicly re-exported from `crates/css/src/lib.rs`, mirroring
> `packages/css/src/index.ts:1,4-7` byte-for-byte.

## Question (original)

`packages/babel-plugin/src/utils/css-builders.ts:4` imports two helpers
from `@compiled/css`:

```ts
import { addUnitIfNeeded, cssAffixInterpolation } from '@compiled/css';
```

JS provenance:

| Function | JS source | Public via |
|---|---|---|
| `addUnitIfNeeded` | `packages/css/src/utils/css-property.ts:118` | `packages/css/src/index.ts:1` |
| `cssAffixInterpolation` | `packages/css/src/utils/css-affix-interpolation.ts:77` | `packages/css/src/index.ts:4-7` |

The §4.4 agent observed that `crates/css/src/lib.rs` only re-exported
the three orchestrators (`transform_css`, `sort`, `generate_compression_map`)
and that the `crates/compiled-css/src/utils/css_property.rs` and
`crates/compiled-css/src/utils/css_affix_interpolation.rs` scaffolds
were header-only stubs with no implementation.

## Resolution

**Verdict:** option 3 of the original list — *deliberately deferred*.
Both functions are part of `@compiled/css@0.19.0`'s public surface
(re-exported by `index.ts`); they belong in `crates/compiled-css`'s
`utils/` (mirroring `packages/css/src/utils/`), and the orchestrator
crate `crates/css` re-exports them so consumers like
`crates/babel-plugin` can import from `css::` the same way the JS file
imports from `@compiled/css`.

What landed (2026-05-04):

1. `crates/compiled-css/src/utils/css_property.rs` — full body.
   - `UNITS: &[&str]` exported (load-bearing order: matches the JS
     `units` array verbatim, including `cm` before `mm` and `s` before
     `ms` so leftmost-first regex alternation in `cssAffixInterpolation`
     resolves identically).
   - `is_unitless_property(name: &str) -> bool` mirrors
     `propertyName in unitless` for all 45 React-style camelCased keys
     (note: `WebkitLineClamp` is the canonical key — lowercase
     `webkitLineClamp` is intentionally NOT unitless; this is upstream
     behaviour, not a typo).
   - `AddUnitValue<'a>` enum (`Null` | `Bool(bool)` | `Str(&str)` |
     `Number(f64)`) covers the JS
     `null | undefined | boolean | string | number` union.
   - `add_unit_if_needed(name, value)` — uses
     `postcss_core::js_number_to_string` for the `${value}px` path so
     the `0.5px` etc. byte-output matches `String(value)` exactly.
2. `crates/compiled-css/src/utils/css_affix_interpolation.rs` — full body.
   - Public types `BeforeInterpolation { css, variable_prefix }` and
     `AfterInterpolation { css, variable_suffix }` — field names use
     snake_case per workspace convention; struct shape matches JS.
   - `css_affix_interpolation(before, after) -> (Before, After)`
     handles the `url()` special case explicitly.
   - **Regex-parity note.** JS source:
     `new RegExp('^(' + units.join('|') + '|"|\'')(;|,|\\n| |\\))?')`.
     Rust's `regex` crate uses leftmost-first alternation matching
     (the same default as ECMAScript), so the JS units order is
     preserved when `UNITS.join("|")` is interpolated. All literal
     chars (`;`, `,`, `\n`, space, `)`) are ASCII; `units` is
     pure-ASCII. No backreferences, no lookaround, no Unicode classes
     — match offsets are byte-identical. `String.prototype.replace`
     with a string needle strips the FIRST occurrence; the Rust port
     uses `replacen(.., "", 1)` to preserve those semantics for any
     pathological input where group 1 might recur (defensive parity —
     current call sites never hit it because the regex is
     `^`-anchored).
3. `crates/css/src/lib.rs` — re-exports the four public symbols
   (`add_unit_if_needed`, `AddUnitValue`, `css_affix_interpolation`,
   `BeforeInterpolation`, `AfterInterpolation`) so `crates/babel-plugin`
   imports them from `css::` (1:1 with JS importing from `@compiled/css`).

## Tests

- `crates/compiled-css/src/utils/css_property.rs` `#[cfg(test)] mod tests` — 9 tests covering null/bool/empty/zero/non-zero/unitless/casing/fractional paths + `js_number_to_string` integration.
- `crates/compiled-css/src/utils/css_affix_interpolation.rs` `#[cfg(test)] mod tests` — **35 tests, 1:1 ports** of `packages/css/src/utils/__tests__/css-affix-interpolation.test.ts` (no JS test left behind).

Verification: `cargo test -p compiled-css --lib` → **163 passed, 0 failed** (was 121 — +42 new). `cargo test -p css --lib` → **27 passed, 0 failed** (re-exports compile clean, no regressions).

## Action for the §4.4 agent

Replace the `unimplemented!("pending crates/css provenance — see CSS_BUILDERS_DEPS.md")` stubs in `crates/babel-plugin/src/utils/css_builders.rs` with:

```rust
use css::{add_unit_if_needed, AddUnitValue, css_affix_interpolation};

// Numeric-literal branch (JS line 528):
let unit_value = add_unit_if_needed(&key, AddUnitValue::Number(prop_value));

// Catch-all template-literal branch (JS line 867):
let (before, after) = css_affix_interpolation(&quasi_raw, &next_quasi_raw);
```

Add `css = { workspace = true }` to `crates/babel-plugin/Cargo.toml` if not already present.
