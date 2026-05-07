# AFM divergence corpus — triage

Source: `tmp_rovodev_css_parity_failing_inputs.log` (5640 lines from
the AFM monorepo where `transformCss(JS)` and `transformCss(Rust via
@compiled/css-native)` produced different bytes on the same input).

Imported via `parity-runner/scripts/import-afm-log.mjs`:

- 5640 log lines → 4266 unique CSS fixtures (after dedup; 1374
  duplicates collapsed).
- Filenames are `<rank>_<sha8>.css` so re-imports are stable and
  same-input → same-name across log captures.

## Headline result

Run: `./target/debug/parity-runner --stage transform-css --corpus
parity-runner/corpus/afm-transform-css`

```
FAIL — 18 of 4266 inputs diverged (JS vs Rust)
```

So **4248 of 4266 (~99.6%)** are byte-clean **in this workspace**
even though they diverged in the AFM monorepo. That gap is
environment skew, not a Rust port bug — the AFM run uses the
real `@atlaskit-tokens` browserslist target, the parity-runner
forces `BROWSERSLIST=chrome 100`. **Do not "fix" those 4248 in
Rust** — they are valid green-here / red-there cases driven by
options the bridge doesn't replay.

JS oracle is deterministic on this corpus (`--determinism` ⇒
4266/4266 stable across two bun spawns), so the 18 remaining
divergences are real Rust-port drift to chase.

## Bisect — 18 surviving fixtures, by first failing stage

Bisect order mirrors `packages/css/src/transform.ts` and is
implemented in `parity-runner/scripts/bisect.sh`.

| First failing stage | Count | Labels |
|---|---|---|
| `cssnano-band` | 7 | 00061, 00762, 00843, 01382, 01414, 02611, 03452 |
| `atomicify-rules` | 5 | 00474, 00475, 00544, 01538, 02556 |
| `transform-css` (assembly only) | 5 | 01794, 01798, 01799, 02527, 03046 |
| `merge-duplicate-at-rules` | 1 | 02521 |

All 18 fall into two byte-pattern groups, suggesting at most ~3
underlying bugs:

### Group A — spurious `-webkit-background-clip` prefix (14 fixtures)

Every fixture whose Rust output contains `-webkit-background-clip:`
where the JS output does not. Lengths differ by 32–47 bytes per
prefix occurrence.

Spans first-failing-stages: `cssnano-band` (7), `transform-css` (5
where autoprefixer-in-isolation passes but the assembled output
diverges), `merge-duplicate-at-rules` (1), and is hidden inside the
3 `atomicify-rules` cases below.

Hypothesis: Rust autoprefixer's `background-clip` hack is firing
when JS would skip it for the configured browserslist (`chrome 100`
should NOT need `-webkit-background-clip`). To localise:

```bash
# Extract just the cssnano-band-failing input and inspect.
diff <(./target/debug/parity-runner --stage cssnano-band \
        --corpus <(printf '%s' "$css") | jq -r .css) \
     <(./target/debug/parity-runner --stage autoprefixer \
        --corpus <(printf '%s' "$css") | jq -r .css)
```

Look at `crates/autoprefixer/src/hacks/background-clip.rs` (or
equivalent) and the corresponding `node_modules/autoprefixer/lib/
hacks/background-clip.js`.

### Group B — atomicify-rules ordering / class-name re-hash (4 fixtures)

00474, 00475, 00544, 01538, 02556 — same total length JS-vs-Rust,
divergence at a `_<hash>` token. This is the Phase 4d CRITICAL
plugin where any iteration-order or hash-input drift renames every
class downstream.

Look at `crates/compiled-css/src/plugins/atomicify_rules.rs` against
`packages/css/src/plugins/atomicify-rules.ts`. Common drift sources:

1. `IndexMap` insertion order vs JS Object insertion order.
2. Hash function input string assembly (every byte counts —
   `@compiled/utils` `hash`).
3. Pseudo-selector handling (00474/00475 hit `>div` and
   `[data-smart-element-link]`).

## Replay

```bash
cd crates
cargo build -p parity-runner

# Whole AFM corpus, ~100s.
./target/debug/parity-runner --stage transform-css \
    --corpus parity-runner/corpus/afm-transform-css

# Bisect a single fixture to its first failing stage.
./parity-runner/scripts/bisect.sh \
    parity-runner/corpus/afm-transform-css/00061_732b78ee.css

# Sanity-check the JS oracle on this corpus.
./target/debug/parity-runner --stage transform-css \
    --corpus parity-runner/corpus/afm-transform-css --determinism
```

After fixing each root cause, re-run the full corpus and the count
should drop by the cluster size (e.g. fixing the
`background-clip` autoprefixer bug should drop 14 of the 18 in one
go).
