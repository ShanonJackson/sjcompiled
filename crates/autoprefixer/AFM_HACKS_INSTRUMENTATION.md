# AFM hacks — instrumentation report

> **Phase A** of `AGENT_5.md`. Empirical answer to "which of the 56
> autoprefixer hacks does AFM's CSS surface actually reach when run
> through `autoprefixer@10.4.14` on AFM's exact `.browserslistrc`?"
> Bounds the Phase B port scope.
>
> **Headline:** five hack classes — `Intrinsic`, `CrossFade`,
> `TextDecoration`, `TextDecorationSkipInk`, `UserSelect`. The other 51
> are out of scope; their prefix tables don't apply to any browser AFM
> targets, so `Prefixes::new` doesn't even instantiate them.

---

## 1. The methodology

Two independent measurements; both confirm the same set of in-scope
hacks.

### 1a. Static analysis — what `Prefixes::new` actually instantiates

Tool: `crates/autoprefixer/_phase_a_scratch/dump_info.mjs`. Loads
`autoprefixer@10.4.14` with AFM's exact browserslist (six `last N
<browser> version` atoms — see §2), constructs a `Prefixes` instance
against `caniuse-lite@1.0.30001766`, then walks `prefixes.add` /
`prefixes.add[*].values` / `prefixes.add.selectors` / `prefixes.remove*`
and reports each prefixer's `constructor.name`. This is the
authoritative signal — if a hack class doesn't appear here, it cannot
fire on any AFM input regardless of CSS content.

Raw output: `_phase_a_scratch/info_dump.txt`.

### 1b. Runtime instrumentation — what actually fires on AFM-shaped CSS

Tool: `crates/autoprefixer/_phase_a_scratch/instrument.mjs`. Wraps
`Prefixer.prototype.process` (catches all Selector/Value/Declaration
hack dispatches via `super.process`), `AtRule.prototype.process`,
`Resolution.prototype.process`, `Supports.prototype.process`, and
`Transition.prototype.{add,remove}`. Each wrapper records the
hack class name + the prop/value being processed + whether the call
returned a non-empty `added` array (= "worked"). Runs the wrapped
processor on a corpus and emits aggregated counts as JSON.

Corpus = 833 CSS files, 86 KB total:
- `crates/parity-runner/corpus/` — 823 atomic-CSS files exercising
  the rest of the pipeline (atomicify-rules, discard-duplicates,
  expand-shorthands, etc.). These are byte-representative of
  `@compiled/css` output, which is exactly what AFM feeds autoprefixer.
- `_phase_a_scratch/afm_synthetic_corpus/` — 10 additional fixtures
  hand-curated to exercise common AFM React UI patterns:
  flexbox toolbars, grid dashboards, form inputs with placeholders &
  appearance, animations & transitions, brand gradients, modals with
  backdrop-filter, text-decoration on links, mask/border-image on
  avatars, intrinsic sizing (fit-content/stretch), print-color-adjust.

Raw output: `_phase_a_scratch/combined_results.json`.

### 1c. What the report does NOT do

- **AFM-pipeline runtime instrumentation.** Per `AGENT_5.md` Method
  option 2, the more accurate measurement would be running the
  instrumented `autoprefixer` inside AFM's actual `jira/` build pipeline
  (mirroring the protocol that produced `BROWSER_LIST_FROM_AFM.md`).
  This requires AFM dependency-engineer coordination. Static analysis
  (1a) is sufficient for bounding the hack-port scope (it answers
  "which hacks CAN ever fire on AFM browsers"), and runtime instrumentation
  on a representative corpus (1b) confirms each in-scope hack is
  actually exercised by realistic input. If AFM's CSS surface evolves
  in a way that changes this set, the static answer (1a) catches it
  immediately on the next browserslist or @compiled/css repin.

---

## 2. AFM browserslist — pinned

Source: `BROWSER_LIST_FROM_AFM.md` (workspace root) and
`crates/browserslist-shim/tests/fixtures/afm/.browserslistrc`. Verbatim
contents:

```
last 2 Edge version
last 2 Firefox version
last 5 Chrome version
last 2 Safari version
last 2 iOS version
last 2 ChromeAndroid version
```

Resolved against `caniuse-lite@1.0.30001766` (14 browser/version atoms):

```
and_chr 144
chrome 144, 143, 142, 141, 140
edge 144, 143
firefox 147, 146
ios_saf 26.2, 26.1
safari 26.2, 26.1
```

Crucially: **no IE, no legacy Safari/Chrome.** Per `autoprefixer.info()`
this surface only needs:

- **6 prefixed properties** (`-webkit-` for `text-decoration`,
  `text-decoration-skip`, `text-decoration-skip-ink`, `text-size-adjust`,
  `user-select`, `box-decoration-break`)
- **6 prefixed values** (`-webkit-`/`-moz-` for `cross-fade`,
  `element`, `fill`, `fill-available`, `fit-content`, `stretch`)
- **0 prefixed selectors** (modern browsers handle `:autofill`,
  `:fullscreen`, `::placeholder`, `:placeholder-shown`, `::file-selector-button`
  natively without prefix)
- **0 at-rule renames** (no `@-webkit-keyframes`, no
  `@-moz-document`, no `@-webkit-viewport`)

This is why the hack-port scope is so narrow.

---

## 3. The in-scope hacks (port these in Phase B)

Five classes. Listed in registration order from
`crates/_vendor/autoprefixer-10.4.14/package/lib/prefixes.js`:

| # | JS source                                       | Class                  | Parent      | LOC | Why AFM hits it                                     |
|---|-------------------------------------------------|------------------------|-------------|----:|-----------------------------------------------------|
| 1 | `lib/hacks/user-select.js`                      | `UserSelect`           | Declaration |  28 | `user-select: none` is universal for buttons / drag handles. AFM emits `-webkit-user-select` for Safari ≤ 16. |
| 2 | `lib/hacks/text-decoration.js`                  | `TextDecoration`       | Declaration |  25 | Non-basic `text-decoration` shorthand values (e.g. `underline solid 2px`) need `-webkit-` for Safari. |
| 3 | `lib/hacks/text-decoration-skip-ink.js`         | `TextDecorationSkipInk`| Declaration |  23 | Claims both `text-decoration-skip` (legacy) AND `text-decoration-skip-ink`; both need `-webkit-` on Safari. |
| 4 | `lib/hacks/cross-fade.js`                       | `CrossFade`            | Value       |  35 | `cross-fade()` value gets `-webkit-cross-fade()`. Rare in AFM but appears in brand decoration. |
| 5 | `lib/hacks/intrinsic.js`                        | `Intrinsic`            | Value       |  61 | Six names: `max-content`, `min-content`, `fit-content`, `fill`, `fill-available`, `stretch`. AFM uses `width: fit-content` heavily for buttons / chips. |

**Total: 172 LOC of hack source.** All five hacks are simple
sub-classes (no shared helpers like `flex-spec.js` or `grid-utils.js`).

### Sanity-test transformation

Confirms the five hacks fire on representative input
(`_phase_a_scratch/sanity_test.mjs`):

```css
/* input */
.fit       { width: fit-content; }
.fill      { width: fill-available; }
.stretch   { width: stretch; }
.user      { user-select: none; }
.text      { text-decoration: underline; }       /* basic — no prefix */
.cross     { background: cross-fade(url(a.png), url(b.png), 50%); }

/* output for AFM browserslist */
.fit       { width: -moz-fit-content; width: fit-content; }
.fill      { width: -webkit-fill-available; width: fill-available; }
.stretch   { width: -webkit-fill-available; width: -moz-available;
             width: stretch; }
.user      { -webkit-user-select: none; user-select: none; }
.text      { text-decoration: underline; }
.cross     { background: -webkit-cross-fade(url(a.png), url(b.png), 50%);
             background: cross-fade(url(a.png), url(b.png), 50%); }
```

(The cross-fade output has a known autoprefixer bug — the URL parens
get re-balanced incorrectly. We must replicate it byte-for-byte per
the `crates/CLAUDE.md` "no work-around" rule.)

---

## 4. The out-of-scope hacks (51 — DO NOT port for AFM)

Static analysis confirms `Prefixes::new` for AFM's browserslist does
NOT instantiate any of these. They cannot fire on AFM input.

### Selector hacks (5) — `prefixes.add.selectors` is empty

| JS source                          | Class                | Why AFM doesn't need it                                   |
|------------------------------------|----------------------|-----------------------------------------------------------|
| `hacks/autofill.js`                | `Autofill`           | `:autofill` natively in Safari ≥ 15, Chrome ≥ 89          |
| `hacks/fullscreen.js`              | `Fullscreen`         | `:fullscreen` natively in Safari ≥ 16.4, Chrome ≥ 71      |
| `hacks/placeholder.js`             | `Placeholder`        | `::placeholder` natively across all AFM targets           |
| `hacks/placeholder-shown.js`       | `PlaceholderShown`   | `:placeholder-shown` natively across all AFM targets       |
| `hacks/file-selector-button.js`    | `FileSelectorButton` | `::file-selector-button` natively in Safari ≥ 16.4         |

### Declaration hacks (38)

All flexbox-spec-2009/2012 hacks (the entire `flex*` and most
`align*`/`justify*` family), grid (IE-grid emulation), animation, etc.
None of these prefix targets are reached by AFM browsers.

| JS source                                | Class                  | Why AFM doesn't need it                                  |
|------------------------------------------|------------------------|----------------------------------------------------------|
| `hacks/align-content.js`                 | `AlignContent`         | `align-content` unprefixed Chrome ≥ 21, Safari ≥ 7       |
| `hacks/align-items.js`                   | `AlignItems`           | unprefixed Chrome ≥ 21, Safari ≥ 7                       |
| `hacks/align-self.js`                    | `AlignSelf`            | unprefixed Chrome ≥ 21, Safari ≥ 7                       |
| `hacks/animation.js`                     | `Animation`            | `animation` unprefixed Chrome ≥ 43, Safari ≥ 9            |
| `hacks/appearance.js`                    | `Appearance`           | `appearance` unprefixed Chrome ≥ 84, Safari ≥ 15.4       |
| `hacks/backdrop-filter.js`               | `BackdropFilter`       | unprefixed Safari ≥ 18, Chrome ≥ 76                       |
| `hacks/background-clip.js`               | `BackgroundClip`       | `text` value unprefixed Safari ≥ 18                       |
| `hacks/background-size.js`               | `BackgroundSize`       | unprefixed everywhere AFM cares about                     |
| `hacks/block-logical.js`                 | `BlockLogical`         | logical block-* unprefixed Chrome ≥ 87, Safari ≥ 14.1     |
| `hacks/border-image.js`                  | `BorderImage`          | unprefixed everywhere AFM cares about                     |
| `hacks/border-radius.js`                 | `BorderRadius`         | unprefixed everywhere                                     |
| `hacks/break-props.js`                   | `BreakProps`           | `break-*` unprefixed Safari ≥ 10.1                        |
| `hacks/filter.js`                        | `Filter`               | unprefixed Safari ≥ 9.1                                   |
| `hacks/flex.js`                          | `Flex`                 | flexbox 2009 spec — IE/old WebKit only                    |
| `hacks/flex-basis.js`                    | `FlexBasis`            | (same)                                                    |
| `hacks/flex-direction.js`                | `FlexDirection`        | (same)                                                    |
| `hacks/flex-flow.js`                     | `FlexFlow`             | (same)                                                    |
| `hacks/flex-grow.js`                     | `FlexGrow`             | (same)                                                    |
| `hacks/flex-shrink.js`                   | `FlexShrink`           | (same)                                                    |
| `hacks/flex-wrap.js`                     | `FlexWrap`             | (same)                                                    |
| `hacks/grid-area.js`                     | `GridArea`             | IE 10/11 grid only                                        |
| `hacks/grid-column-align.js`             | `GridColumnAlign`      | (same)                                                    |
| `hacks/grid-end.js`                      | `GridEnd`              | (same)                                                    |
| `hacks/grid-row-align.js`                | `GridRowAlign`         | (same)                                                    |
| `hacks/grid-row-column.js`               | `GridRowColumn`        | (same)                                                    |
| `hacks/grid-rows-columns.js`             | `GridRowsColumns`      | (same)                                                    |
| `hacks/grid-start.js`                    | `GridStart`            | (same)                                                    |
| `hacks/grid-template.js`                 | `GridTemplate`         | (same)                                                    |
| `hacks/grid-template-areas.js`           | `GridTemplateAreas`    | (same)                                                    |
| `hacks/image-rendering.js`               | `ImageRendering`       | `pixelated` unprefixed everywhere AFM cares about         |
| `hacks/inline-logical.js`                | `InlineLogical`        | logical inline-* unprefixed Chrome ≥ 87, Safari ≥ 14.1    |
| `hacks/justify-content.js`               | `JustifyContent`       | unprefixed Chrome ≥ 21, Safari ≥ 7                        |
| `hacks/mask-border.js`                   | `MaskBorder`           | (still flagged in modern Safari, but AFM doesn't use it)  |
| `hacks/mask-composite.js`                | `MaskComposite`        | (same)                                                    |
| `hacks/order.js`                         | `Order`                | unprefixed Chrome ≥ 21, Safari ≥ 7                        |
| `hacks/overscroll-behavior.js`           | `OverscrollBehavior`   | unprefixed Chrome ≥ 63, Safari ≥ 16                       |
| `hacks/place-self.js`                    | `PlaceSelf`            | unprefixed Chrome ≥ 59, Safari ≥ 11                       |
| `hacks/print-color-adjust.js`            | `PrintColorAdjust`     | `color-adjust` rename — already unprefixed in AFM Safari  |
| `hacks/text-emphasis-position.js`        | `TextEmphasisPosition` | `-webkit-text-emphasis` only needed Safari < 7            |
| `hacks/transform-decl.js`                | `TransformDecl`        | `transform` unprefixed Chrome ≥ 36, Safari ≥ 9            |
| `hacks/writing-mode.js`                  | `WritingMode`          | unprefixed Chrome ≥ 48, Safari ≥ 10.1                     |

(That's 41 entries — 5 selector + 36 declaration hacks. Plus 2 helpers
below = 51 out-of-scope total. The Value-bucket hacks not on the
in-scope list are below.)

### Value hacks (6) — only `Intrinsic`/`CrossFade` are loaded

| JS source                  | Class            | Why AFM doesn't need it                                                      |
|----------------------------|------------------|------------------------------------------------------------------------------|
| `hacks/display-flex.js`    | `DisplayFlex`    | `display: flex/inline-flex` unprefixed Chrome ≥ 29, Safari ≥ 9               |
| `hacks/display-grid.js`    | `DisplayGrid`    | `display: grid` unprefixed Chrome ≥ 57, Safari ≥ 10.1                        |
| `hacks/filter-value.js`    | `FilterValue`    | `filter: ...` value-bucket — `-webkit-filter` unneeded for AFM Safari        |
| `hacks/gradient.js`        | `Gradient`       | linear/radial gradients unprefixed Chrome ≥ 26, Safari ≥ 6.1, with old syntax cleanup unneeded for AFM |
| `hacks/image-set.js`       | `ImageSet`       | `image-set()` still prefixed in some AFM Safari targets BUT the data table doesn't trigger for AFM's resolved set |
| `hacks/pixelated.js`       | `Pixelated`      | `image-rendering: pixelated` unprefixed everywhere AFM cares                 |

### Helpers (2) — referenced only by the above

| JS source                  | Why AFM doesn't need it                                                  |
|----------------------------|--------------------------------------------------------------------------|
| `hacks/flex-spec.js`       | Only consumed by `flex*`/`align-content` — none in scope                  |
| `hacks/grid-utils.js`      | Only consumed by `grid-*` — none in scope                                 |

### Other base-class entries that show up in `prefixes.add` but are NOT hacks

These are constructed for AFM but resolve to a base class (`Declaration`
or `Value`), not a hack subclass. The base classes are already implemented
(per AGENT_1's `prefixes.rs::HackRegistry` framework). No port action
needed:

- `box-decoration-break` → base `Declaration` (AFM Safari needs `-webkit-`)
- `text-size-adjust` → base `Declaration` (AFM Safari needs `-webkit-`)
- `element` value (across `background`, `background-image`, `border-image`,
  `content`, `list-style`, `mask`) → base `Value` (Firefox needs `-moz-`)

### Always-instantiated base infrastructure (NOT hacks)

These are base classes that always exist for any non-empty browserslist.
Owned by other agents:

| Class       | Owner    | Notes                                                                |
|-------------|----------|----------------------------------------------------------------------|
| `Transition`| AGENT_3  | `prefixes.transition` always exists. For AFM, only `-webkit-` prefix; only fires when transitioning a property that itself needs a prefix (e.g. `transition: -webkit-mask 0.2s`). On the corpus, fired 19× in `add` and 19× in `remove`. |
| `Supports`  | AGENT_2  | `prefixes.add['@supports']` always exists. Rewrites prefixed names inside `@supports (...)` parens. Fired 29× across the corpus on `@supports`-bearing fixtures. |
| `Resolution`| (foundation) | Only loaded when `prefixes.add['@resolution']` exists; for AFM, NOT loaded (`@-webkit-min-device-pixel-ratio` no longer needed). |

---

## 5. Runtime instrumentation — the raw counts

Per `_phase_a_scratch/combined_results.json`, on 803 processed files
(86,302 bytes total):

| Class                  | Dispatch              | "Worked" (mutated state) | "Offered" (entered process) |
|------------------------|-----------------------|-------------------------:|----------------------------:|
| Supports               | supports.process      |                       29 |                          29 |
| Transition             | transition.add        |                       19 |                          19 |
| Transition             | transition.remove     |                       19 |                          19 |
| TextDecoration         | process               |                        3 |                           8 |
| TextDecorationSkipInk  | process               |                        1 |                           1 |
| UserSelect             | process               |                        1 |                           2 |
| Intrinsic              | process (×4 names)    |                        0 |                         404 |
| CrossFade              | process               |                        0 |                         248 |
| Value (base, `element`)| process               |                        0 |                         248 |

**Caveat about "worked = 0" on Value-bucket hacks.** `Value.add()`
returns `undefined` even when it mutates `decl._autoprefixerValues`;
the actual node-level mutation happens later in `Value.save()` (called
from `processor.js`). My instrumentation's "worked = `Array.isArray(ret)
&& ret.length > 0`" heuristic correctly captures Declaration hacks but
undercounts Value-bucket hacks. Verified independently in
`sanity_test.mjs` that `width: fit-content` / `width: fill-available` /
`width: stretch` / `cross-fade(...)` all DO mutate output for AFM
browsers. So the "0 worked" rows for Intrinsic / CrossFade are an
artefact of the heuristic, NOT a signal those hacks are unused.

**Implication for Phase B port.** In-scope set is bounded by the
static analysis (§3). The runtime numbers mostly confirm the static
read; the only surprise was that the synthetic corpus didn't happen to
mutate `Intrinsic`/`CrossFade` decls in a way the (return-value-based)
heuristic could see — but these hacks DO fire as proven by the sanity
test, and the static analysis confirms they're loaded. They MUST be
ported.

---

## 6. What this report does NOT prescribe

- **Whether AFM uses `Intrinsic` heavily enough to be on a hot path.**
  Possibly significant: AFM's atomic CSS uses `width: fit-content` for
  badges/chips, which is one Intrinsic invocation per atomic class. If
  AFM has 200+ button variants this could be hundreds of dispatches.
  Doesn't change the port decision (still need to port it cleanly), but
  worth knowing for performance tuning later.
- **Whether AFM ever uses `cross-fade()`.** The synthetic corpus
  includes one fixture (`05_gradients_filters.css`) with a `cross-fade`
  decl that triggers the hack. AFM may or may not use it in practice —
  static analysis says it's loaded, so we must port regardless.
- **What happens if AFM's browserslist evolves.** If AFM widens to
  e.g. `last 10 Safari versions` to support older Safari, additional
  hacks (especially flex-spec-2012 hacks, gradient old-syntax cleanup)
  could become in-scope. Re-run this report whenever
  `crates/browserslist-shim/tests/fixtures/afm/.browserslistrc` changes
  (its SHA256 is asserted in `crates/browserslist-shim/tests/afm_parity.rs`,
  so a change is loud).

---

## 7. Reproducing this report

```
cd crates/autoprefixer/_phase_a_scratch
bun install                              # one-time setup, pulls
                                         # autoprefixer@10.4.14 +
                                         # browserslist@4.24.2 +
                                         # caniuse-lite@1.0.30001766
bun run dump_info.mjs                    # static analysis
bun run instrument.mjs ./afm_synthetic_corpus/ ../../parity-runner/corpus/
bun run sanity_test.mjs                  # verify the in-scope hacks
                                         # actually mutate output
```

The scratch directory IS gitignored-equivalent (under `crates/autoprefixer/`,
not under `src/`). Keep it around — re-running on a future
caniuse-lite repin is the only catch for the "AFM browserslist surface
shifted under us" failure mode.
