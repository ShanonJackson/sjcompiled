# Hacks port — split contract

This file is the parallel-agent's checklist + contract for porting the 58
files under `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/` to
the Rust crate at `crates/autoprefixer/src/hacks/`.

> **Cardinal rule (from `crates/STATUS.md`):** a session must take a
> unit from 0 → 100% byte-clean. Half-done hacks become silent
> byte-drift hazards. Pick a hack from the table below, take it 0 →
> 100%, mark the row, move on.

## Boundaries (do not cross)

You own everything under `crates/autoprefixer/src/hacks/`. The base
classes you subclass (`Declaration`, `Value`, `Selector`, `AtRule`,
`Resolution`, `Supports`, `Transition`) live one directory up under
`crates/autoprefixer/src/`. The foundation/core agent owns those — DO
NOT edit them. If a hack needs a method that isn't on the base trait,
**stop and file a note here**, do not patch the base class yourself.

The one shared file you may edit is
`crates/autoprefixer/src/prefixes.rs` — specifically the
`register_hacks()` block. Each hack registers itself there with a
single `register::<MyHack>()` line. Add yours in the **alphabetical
position matching the JS source** (preserves byte-for-byte registration
order, which affects `Object.keys` iteration order downstream).

## Status — base classes are READY

As of this checkpoint, all five base classes have real method bodies
and passing unit tests:

| Base class    | File                | Tests | Notes |
|---------------|---------------------|------:|-------|
| `Prefixer`    | `prefixer.rs`       |     5 | `parent_prefix` walks via `walk_up_with`, caches via `Node.attrs`, `clone_node` strips via `clone_without` |
| `AtRuleBase`  | `at_rule.rs`        |     4 | full `add` + `process` (incl. path-shift handling) |
| `ValueBase`   | `value.rs`          |     7 | full `check` / `regexp` / `replace` / `value` / `add` / `old`; `_autoprefixerValues` cache via `AttrValue::StringMap` |
| `SelectorBase`| `selector.rs`       |     6 | full `prefixed` / `regexp` / `replace` / `prefixeds` / `already` (sibling-walk via `parent_nodes`) / `add` / `old` |
| `DeclarationBase` | `declaration.rs` |    7 | full `prefixed` / `set` / `otherPrefixes` / `needCascade` / `maxPrefixed` / `calcBefore` / `insert` / `add` / `process`; cascade memoised via `_autoprefixerCascade` / `_autoprefixerMax` |
| `ResolutionBase` | `resolution.rs`  |    4 | full `prefixName` / `prefixQuery` / `clean` / `process` (uses `fraction_js`) |
| `Browsers`    | `browsers.rs`       |     4 | full static `prefixes()` / `withPrefix` / instance `prefix(browser)` / `isSelected` |

**Hacks agent: you can start now.** Pick a hack, port it, register it in
`crates/autoprefixer/src/prefixes.rs::register_hacks` (the BEGIN/END
block), tick the row below.

Still-stubbed (NOT base-class — won't affect hacks): `supports.rs`,
`transition.rs` (heavy classes hacks rarely subclass), `processor.rs`,
`info.rs`, `autoprefixer.rs`, `data/prefixes.rs`. These don't need to
exist before you can write a hack.

## Trait surface

Every hack is a struct that owns a `*Base` and implements the
appropriate base type. Pattern (mirroring JS `class AlignContent
extends Declaration`):

```rust
//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/align-content.js`.

use crate::declaration::DeclarationBase;
use crate::prefixer::{Prefixer, PrefixerBase};

pub struct AlignContent {
    base: DeclarationBase,
}

impl AlignContent {
    pub const NAMES: &'static [&'static str] = &["align-content", "flex-line-pack"];
    pub const OLD_VALUES: &'static [(&'static str, &'static str)] = &[
        ("flex-end", "end"),
        ("flex-start", "start"),
        ("space-between", "justify"),
        ("space-around", "distribute"),
    ];

    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self { base: DeclarationBase::new(name, prefixes, all_id) }
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let (spec, p) = crate::hacks::flex_spec::flex_spec(prefix);
        if spec == 2012 { format!("{p}flex-line-pack") }
        else { self.base.prefixed(prop, &p) }
    }

    fn normalize(&self) -> &'static str { "align-content" }
}

impl Prefixer for AlignContent {
    fn base(&self) -> &PrefixerBase { &self.base.prefixer }
    fn base_mut(&mut self) -> &mut PrefixerBase { &mut self.base.prefixer }
    fn add(&mut self, node: &mut Node, prefix: &str, prefixes: &[String]) -> Option<()> {
        // ... port body of declaration.js::add + the AlignContent overrides
        self.base.add(node, prefix, prefixes)
    }
}
```

The exact base-class trait names + helper-method signatures will be
finalized when the foundation agent lands `declaration.rs` /
`value.rs` / `selector.rs` / `at_rule.rs`. Until then, treat the
above as a sketch.

## File-by-file port checklist

| JS file                         | Parent class | LOC  | Status |
|---------------------------------|--------------|-----:|--------|
| align-content.js                | Declaration  |   49 | TODO |
| align-items.js                  | Declaration  |   46 | TODO |
| align-self.js                   | Declaration  |   56 | TODO |
| animation.js                    | Declaration  |   17 | TODO |
| appearance.js                   | Declaration  |   23 | TODO |
| autofill.js                     | Selector     |   26 | TODO |
| backdrop-filter.js              | Declaration  |   20 | TODO |
| background-clip.js              | Declaration  |   24 | TODO |
| background-size.js              | Declaration  |   23 | TODO |
| block-logical.js                | Declaration  |   40 | TODO |
| border-image.js                 | Declaration  |   15 | TODO |
| border-radius.js                | Declaration  |   40 | TODO |
| break-props.js                  | Declaration  |   63 | TODO |
| cross-fade.js                   | Value        |   35 | TODO |
| display-flex.js                 | Value        |   65 | TODO |
| display-grid.js                 | Value        |   21 | TODO |
| file-selector-button.js         | Selector     |   26 | TODO |
| filter-value.js                 | Value        |   14 | TODO |
| filter.js                       | Declaration  |   19 | TODO |
| flex-basis.js                   | Declaration  |   39 | TODO |
| flex-direction.js               | Declaration  |   72 | TODO |
| flex-flow.js                    | Declaration  |   53 | TODO |
| flex-grow.js                    | Declaration  |   30 | TODO |
| flex-shrink.js                  | Declaration  |   39 | TODO |
| flex-spec.js                    | (helper)     |   19 | TODO |
| flex-wrap.js                    | Declaration  |   19 | TODO |
| flex.js                         | Declaration  |   54 | TODO |
| fullscreen.js                   | Selector     |   20 | TODO |
| gradient.js                     | Value        |  448 | TODO |
| grid-area.js                    | Declaration  |   34 | TODO |
| grid-column-align.js            | Declaration  |   28 | TODO |
| grid-end.js                     | Declaration  |   52 | TODO |
| grid-row-align.js               | Declaration  |   28 | TODO |
| grid-row-column.js              | Declaration  |   33 | TODO |
| grid-rows-columns.js            | Declaration  |  125 | TODO |
| grid-start.js                   | Declaration  |   33 | TODO |
| grid-template-areas.js          | Declaration  |   84 | TODO |
| grid-template.js                | Declaration  |   69 | TODO |
| grid-utils.js                   | (helper)     | 1113 | TODO |
| image-rendering.js              | Declaration  |   48 | TODO |
| image-set.js                    | Value        |   18 | TODO |
| inline-logical.js               | Declaration  |   34 | TODO |
| intrinsic.js                    | Value        |   61 | TODO |
| justify-content.js              | Declaration  |   54 | TODO |
| mask-border.js                  | Declaration  |   38 | TODO |
| mask-composite.js               | Declaration  |   88 | TODO |
| order.js                        | Declaration  |   42 | TODO |
| overscroll-behavior.js          | Declaration  |   33 | TODO |
| pixelated.js                    | Value        |   34 | TODO |
| place-self.js                   | Declaration  |   32 | TODO |
| placeholder-shown.js            | Selector     |   17 | TODO |
| placeholder.js                  | Selector     |   33 | TODO |
| print-color-adjust.js           | Declaration  |   25 | TODO |
| text-decoration-skip-ink.js     | Declaration  |   23 | TODO |
| text-decoration.js              | Declaration  |   25 | TODO |
| text-emphasis-position.js       | Declaration  |   14 | TODO |
| transform-decl.js               | Declaration  |   79 | TODO |
| user-select.js                  | Declaration  |   28 | TODO |
| writing-mode.js                 | Declaration  |   42 | TODO |

Suggested order: start with the smallest Declaration-parented hacks
(`animation`, `appearance`, `border-image`, `filter`,
`text-emphasis-position`) — these exercise the base trait without
leaning on subclass-specific helpers, and validate the
`PrefixerBase`+`DeclarationBase` plumbing the foundation agent landed.

## Registration

In `crates/autoprefixer/src/prefixes.rs`, look for the
`fn register_hacks(reg: &mut HackRegistry)` function. Append:

```rust
reg.register::<crate::hacks::align_content::AlignContent>();
```

Keep the order alphabetical by JS filename.

## Path-shift gotcha (read this before writing any insert loop)

JS holds a node reference across `parent.insertBefore(node, cloned)` —
the reference auto-follows when the original's index shifts. We use
*index paths*. Each successful `insert_before_at_path(root, path, ...)`
shifts the original's index up by 1 because the clone is spliced at
the original's slot. The path becomes stale the moment the insert
returns.

If your hack's `add` (or any method that calls
`insert_before_at_path` in a loop) iterates multiple prefixes:

```rust
let mut current_path = path.to_vec();
for prefix in &prefixes {
    if self.add(root, &current_path, prefix).is_some() {
        if let Some(last) = current_path.last_mut() { *last += 1; }
    }
}
```

The bug is silent: tests that only insert one prefix won't catch it;
tests that insert two or more will. See `at_rule.rs::process` for the
canonical example.

## Don't re-port these

`flex-spec.js` and `grid-utils.js` aren't classes — they're shared
helpers. Port them as plain functions in `hacks/flex_spec.rs` /
`hacks/grid_utils.rs`. Leave them as `pub(crate)` — they're not
registered.
