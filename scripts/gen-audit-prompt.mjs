#!/usr/bin/env node
// Generate audit prompts for AFM-repinned packages.
//
// The Rust ports under `crates/` were originally written against the
// versions pinned in `REFERENCE_LOCK_FILE/yarn.lock` (the upstream
// `compiled` repo's lockfile). We later discovered that AFM/JIRA — the
// real consumer of the Rust port — installs `@compiled/css@0.19.0`
// resolved against a different dependency graph. AFM's resolution wins;
// see `AFM_MONOREPO_DEPENDENCIES_MORE.md` and `crates/PARITY_VERSIONS.md`.
//
// Several packages drifted between the lockfile we ported against and
// what AFM actually installs. Each drifted package needs a full
// source-tree audit + port of any non-cosmetic delta. This script
// emits the per-package agent prompt for that audit.
//
// Usage:
//   node scripts/gen-audit-prompt.mjs <package-name>     # print one prompt to stdout
//   node scripts/gen-audit-prompt.mjs --list             # list known packages
//   node scripts/gen-audit-prompt.mjs --all              # write one .md per package
//                                                       # into ./scripts/audit-prompts/
//   node scripts/gen-audit-prompt.mjs --all --stdout     # dump all prompts to stdout
//   node scripts/gen-audit-prompt.mjs <name> --out file  # write one prompt to a file
//
// Three prompt kinds are generated, picked per-package via `kind`:
//   - "version-drift"     — version differs between lockfile and AFM;
//                           audit walks the diff and ports non-cosmetic
//                           deltas into the existing Rust crate.
//   - "no-drift-reaudit"  — version matches AFM; audit walks the AFM
//                           source vs the existing Rust port to catch
//                           mistakes from the original port.
//   - "local-plugins"     — packages/css/src/plugins/* re-audit against
//                           the @compiled/css@0.19.0 source tree.

import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(SCRIPT_DIR, '..');

// ---------------------------------------------------------------------------
// Package registry. One entry per agent dispatch.
// ---------------------------------------------------------------------------
//
// Field reference:
//   kind:           one of "version-drift", "no-drift-reaudit",
//                   "local-plugins". Picks the prompt template.
//   was:            version string we originally ported against (the
//                   REFERENCE_LOCK_FILE/yarn.lock pin).
//   now:            AFM-resolved version (the new pin per
//                   AFM_MONOREPO_DEPENDENCIES_MORE.md).
//   vendorOld:      filesystem path to the OLD source. Usually
//                   `crates/_vendor/<pkg>-<was>/package/` for
//                   version-drift. May be null if not vendored.
//   bunCacheNew:    filesystem path to the NEW source after
//                   `bun install`. Usually
//                   `node_modules/.bun/<pkg>@<now>*/node_modules/<pkg>/`.
//                   The trailing `*` covers bun's hash suffix on the
//                   directory name — agents should glob it.
//   rustCrate:      target crate to apply ports into.
//   sourceSubdir:   the subdirectory inside the package to walk for
//                   the source-tree diff. Defaults to "" (root).
//                   For postcss it's "lib/", for postcss-selector-parser
//                   it's "dist/", etc.
//   knownDeltas:    short bullet list of already-known non-cosmetic
//                   changes. The agent uses these as anchors but must
//                   still do a full diff to catch the rest.
//   verifyGates:    the parity-runner stages whose corpus must remain
//                   byte-clean after the port lands. Pre-screened to
//                   only include stages that exercise this package.
//   reportPath:     where to write the audit findings doc.
//   notes:          any additional context (patch-DOWN warnings, scaffold
//                   status, etc.). Inserted into the prompt.
//   ownerFiles:     for version-drift kind, the headline files in the
//                   package source. Listed so the agent doesn't miss
//                   anything obvious during the walk.

const PACKAGES = {
    // ----- Tier 1: version drift between REFERENCE_LOCK_FILE and AFM -----

    'postcss': {
        kind: 'version-drift',
        was: '8.4.31',
        now: '8.5.6',
        vendorOld: null, // 8.4.31 may not be vendored — fetch via npm pack if needed
        bunCacheNew: 'node_modules/.bun/postcss@8.5.6/node_modules/postcss/',
        rustCrate: 'crates/postcss-core/',
        sourceSubdir: 'lib/',
        ownerFiles: [
            'parser.js', 'tokenize.js', 'stringifier.js', 'container.js',
            'root.js', 'atrule.js', 'rule.js', 'declaration.js',
            'comment.js', 'node.js', 'list.js', 'css-syntax-error.js',
            'lazy-result.js',
        ],
        knownDeltas: [
            'postcss-core agent previously claimed "cosmetic only" — claim was made before the audit standard was tightened. RE-VERIFY with full file-by-file walk; do not take the prior claim as ground truth.',
            'Empirical diff harness at `crates/_vendor/test-postcss-versions/` (built by the postcss-core agent) compared `parse → stringify` round-trips and found byte-identical output across 26 raw round-trips and 30 plugin × input pairs. Use that harness as ONE input to your audit; do not use it as the SOLE input.',
        ],
        verifyGates: [
            'postcss-core-roundtrip',
            // postcss-core is the foundation — every plugin stage exercises it
            'discard-empty-rules', 'discard-duplicates', 'extract-stylesheets',
            'parent-orphaned-pseudos', 'increase-specificity',
            'merge-duplicate-at-rules', 'normalize-current-color',
            'sort-atomic-style-sheet', 'atomicify-rules', 'expand-shorthands',
            'npm-postcss-discard-duplicates', 'postcss-nested',
            'postcss-normalize-whitespace', 'postcss-discard-comments',
            'postcss-normalize-string', 'postcss-normalize-positions',
            'postcss-normalize-timing-functions', 'postcss-normalize-url',
            'sort',
        ],
        reportPath: 'crates/_vendor/POSTCSS_8.4.31_TO_8.5.6_AUDIT.md',
        notes: 'postcss-core is the load-bearing foundation. Every plugin port depends on its AST shape, raws preservation, stringifier output, and number formatting being byte-identical. A missed change here invalidates EVERY downstream parity claim. This audit is the highest priority of the bunch.',
    },

    'postcss-selector-parser': {
        kind: 'version-drift',
        was: '6.0.13',
        now: '6.1.2',
        vendorOld: 'crates/_vendor/postcss-selector-parser-6.0.13/package/',
        bunCacheNew: 'node_modules/.bun/postcss-selector-parser@6.1.2/node_modules/postcss-selector-parser/',
        rustCrate: 'crates/postcss-selector-parser/',
        sourceSubdir: 'dist/',
        ownerFiles: [
            'parser.js', 'processor.js', 'tokenize.js', 'index.js',
            'sortAscending.js', 'tokenTypes.js',
            'selectors/*.js', 'util/*.js',
        ],
        knownDeltas: [
            '`parser.js` line ~488: new clause treats `closeParenthesis` as a comma-like terminator alongside `tokens.comma`. Affects boundary detection inside `:is()`, `:where()`, `:not()`, `:has()`, `:matches()`. Different boundary → different raws attachment → different stringified bytes → different hash.',
            '`parser.js`: `sourceIndex: …` field added on a few node initializations (commas, pseudos). Diagnostic surface only; not stringified today. Mirror the addition anyway — downstream plugin ports may read the field later.',
        ],
        verifyGates: [
            'postcss-core-roundtrip',
            'sort-atomic-style-sheet',
            'parent-orphaned-pseudos',
            'increase-specificity',
            'atomicify-rules',
            'sort',
        ],
        reportPath: 'crates/_vendor/POSTCSS_SELECTOR_PARSER_6.0.13_TO_6.1.2_AUDIT.md',
        notes: 'A separate agent may already be working on this. If you see an in-progress audit doc at the report path, append to it rather than overwriting.',
    },

    'browserslist': {
        kind: 'version-drift',
        was: '4.24.4',
        now: '4.24.2',
        vendorOld: 'crates/_vendor/browserslist-4.24.4/package/',
        bunCacheNew: 'node_modules/.bun/browserslist@4.24.2+*/node_modules/browserslist/',
        rustCrate: 'crates/browserslist-shim/',
        sourceSubdir: '',
        ownerFiles: [
            'index.js', 'node.js', 'parse.js', 'browser.js',
            'error.js',
        ],
        knownDeltas: [
            '`index.js`: 4.24.4 wraps query parsing in a `parseCache` keyed by `JSON.stringify(queries)` and adds `needsPath` plumbing so `context.path` is only set when a query needs it. 4.24.2 calls `parse(QUERIES, queries)` directly with no cache and always passes `path: opts.path`. Cache infra only — but verify no semantic difference reaches resolved query results.',
            '`index.js` Firefox ESR resolution: 4.24.4 returns `[\'firefox 128\']`; 4.24.2 returns `[\'firefox 115\', \'firefox 128\']`. **Byte-affecting** for any consumer that hits the `Firefox ESR` query — affects autoprefixer prefix decisions and ~5 cssnano plugins. The Rust shim wraps `oxc_browserslist` v3, which has its own bundled snapshot — verify that calling `oxc_browserslist::resolve(&["Firefox ESR"], ...)` returns the 4.24.2-style two-version list, NOT just `firefox 128`. If oxc_browserslist returns the wrong list, override locally.',
            '`node.js`: significant cache infra changes (4.24.4 added `statCache`, `configPathCache`, `parseConfigCache`; `eachParent` signature changed). Internal only as far as we can tell, but enumerate every change.',
        ],
        verifyGates: [
            // browserslist isn't directly exercised by current parity stages
            // (autoprefixer is in progress). Smoke-test by adding adversarial
            // corpus entries that thread Firefox-ESR-targeted browserslist
            // queries through the autoprefixer-aware cssnano plugins once
            // those parity gates are live. For NOW the verification is unit
            // tests inside `crates/browserslist-shim/`.
            'postcss-core-roundtrip',
            'sort',
        ],
        reportPath: 'crates/_vendor/BROWSERSLIST_4.24.4_TO_4.24.2_AUDIT.md',
        notes: 'Patch DOWN direction is unusual (4.24.4 → 4.24.2). Confirm with the AFM dependency engineer that 4.24.2 isn\'t a transient AFM mistake before sinking serious port work. If they confirm, the rest of this audit applies. If they retract, escalate to repin to 4.24.4 instead.',
    },

    'caniuse-lite': {
        kind: 'version-drift',
        dataOnly: true,
        was: '1.0.30001690',
        now: '1.0.30001766',
        vendorOld: 'crates/_vendor/caniuse-lite-1.0.30001690/package/',
        bunCacheNew: 'crates/_vendor/caniuse-lite-1.0.30001766/package/', // already vendored
        rustCrate: 'crates/caniuse-db/',
        consumedBy: '`crates/caniuse-db/scripts/snapshot.js` (Node-side codegen) → `crates/caniuse-db/data/features.snapshot.json` (consumed by `caniuse-db::features::*`). Also `crates/autoprefixer/build.rs` (which `require()`s caniuse-lite at codegen time to expand the data/prefixes.js table).',
        sourceSubdir: 'data/',
        ownerFiles: [
            'data/agents.js', 'data/browsers.js', 'data/browserVersions.js',
            'data/features.js', 'data/features/*.js',
            'data/regions/*.js',
        ],
        knownDeltas: [
            'Feature count: 579 → 582. Three NEW features added between snapshots — identify which.',
            '76 monthly snapshots forward. Many existing features have updated agent support tables (new browser versions added/removed; partial-support flags flipped). Each change is a potential autoprefixer prefix-decision drift.',
            'The Rust runtime snapshot has already been regenerated at `crates/caniuse-db/data/features.snapshot.json`. The audit verifies the **content** matches expectations — do NOT re-vendor on top of work already done.',
            'Verify `oxc_browserslist` v3 doesn\'t bundle a different caniuse-lite snapshot internally and silently use it. If it does, the shim must override to use ours.',
        ],
        verifyGates: [
            // Caniuse-lite is data — the verification is per-feature checks
            // that downstream consumers (autoprefixer, the browserslist-aware
            // cssnano plugins) see the same support matrix as the JS oracle.
            // The integration parity gates for autoprefixer are not yet live;
            // for now the verification is via `crates/caniuse-db` and
            // `crates/caniuse-api` unit tests + spot-check assertions.
            'postcss-core-roundtrip',
        ],
        reportPath: 'crates/_vendor/CANIUSE_LITE_1.0.30001690_TO_1.0.30001766_AUDIT.md',
        notes: 'DATA-ONLY — there is no JS source code to port, just data tables. The audit task is: (1) enumerate which features changed support tables; (2) for each changed feature, check whether any `crates/autoprefixer/src/data/prefixes.rs` entry references it; (3) re-run the autoprefixer codegen if needed (build.rs handles this automatically when `bun install` resolves the new caniuse-lite); (4) pick a sample of high-traffic features (flexbox, grid, position-sticky, mask, aspect-ratio, container queries, :has, transforms, gradients) and confirm Rust-side `caniuse_db::feature("X")` returns the same support matrix as Node-side `require("caniuse-lite/data/features/X.js")` post-unpack.',
    },

    'electron-to-chromium': {
        kind: 'version-drift',
        dataOnly: true,
        notYetConsumed: true,
        was: '1.5.76',
        now: '1.5.41',
        vendorOld: 'crates/_vendor/electron-to-chromium-1.5.76/',
        bunCacheNew: 'node_modules/.bun/electron-to-chromium@1.5.41/node_modules/electron-to-chromium/',
        rustCrate: 'crates/caniuse-db/',
        consumedBy: '**Currently NOT consumed by any Rust code.** Mentioned only in `crates/caniuse-db/Cargo.toml` description string and `crates/PARITY_VERSIONS.md` table. The package is vendored as a forward-compatibility pin for future browserslist/autoprefixer work that resolves `electron N` queries — `oxc_browserslist` v3 wraps its own bundled mappings today, so we don\'t reach the JS data tables.',
        sourceSubdir: '',
        ownerFiles: [
            'chromium-versions.js', 'chromium-versions.json',
            'versions.js', 'versions.json',
            'full-chromium-versions.js', 'full-chromium-versions.json',
            'full-versions.js', 'full-versions.json',
        ],
        knownDeltas: [
            'Patch DOWN direction (1.5.76 → 1.5.41). 35 patch versions removed — likely Chromium-version mappings for releases that did not exist when 1.5.41 was published.',
            'No Rust source consumes this data today. The audit is a documentation/forward-pin sanity check, not a port.',
        ],
        verifyGates: [
            'postcss-core-roundtrip',
        ],
        reportPath: 'crates/_vendor/ELECTRON_TO_CHROMIUM_1.5.76_TO_1.5.41_AUDIT.md',
        notes: '**LOW PRIORITY** — `grep -rn "electron-to-chromium\\|electron_to_chromium" crates/` returns ONE hit, in `crates/caniuse-db/Cargo.toml`\'s description string. Nothing actually reads the data. The audit task is reduced to: (1) re-vendor `crates/_vendor/electron-to-chromium-1.5.41/` for future use; (2) document in the report that no Rust consumer exists yet so no port is required; (3) flag the hand-off point — when autoprefixer\'s `Browsers::new` call site lands, it WILL need this data, and the future agent should re-run this audit then.',
    },

    'node-releases': {
        kind: 'version-drift',
        dataOnly: true,
        notYetConsumed: true,
        was: '2.0.19',
        now: '2.0.18',
        vendorOld: 'crates/_vendor/node-releases-2.0.19/',
        bunCacheNew: 'node_modules/.bun/node-releases@2.0.18/node_modules/node-releases/',
        rustCrate: 'crates/caniuse-db/',
        consumedBy: '**Currently NOT consumed by any Rust code.** Mentioned only in `crates/caniuse-db/Cargo.toml` description string. Vendored as a forward-compatibility pin for browserslist `node N` query resolution; `oxc_browserslist` handles node version queries internally today.',
        sourceSubdir: 'data/',
        ownerFiles: [
            'data/processed/envs.json',
            'data/release-schedule/release-schedule.json',
        ],
        knownDeltas: [
            'Patch DOWN (2.0.19 → 2.0.18). One patch version removed.',
            'No Rust source consumes this data today. The audit is a documentation/forward-pin sanity check, not a port.',
        ],
        verifyGates: [
            'postcss-core-roundtrip',
        ],
        reportPath: 'crates/_vendor/NODE_RELEASES_2.0.19_TO_2.0.18_AUDIT.md',
        notes: '**LOW PRIORITY** — `grep -rn "node-releases\\|node_releases" crates/` returns ONE hit, in `crates/caniuse-db/Cargo.toml`\'s description string. Nothing actually reads the data, AND Compiled\'s CSS pipeline rarely queries node versions (browserslist queries target browsers, not server runtimes). Audit reduces to: (1) re-vendor `crates/_vendor/node-releases-2.0.18/` for future use; (2) document in the report that no Rust consumer exists yet so no port is required; (3) flag the hand-off point for whichever future agent first reaches a `node N` query path. **Consider deferring this audit indefinitely** unless a real consumer appears.',
    },

    'colord': {
        kind: 'version-drift',
        was: '2.9.1',
        now: '2.9.3',
        vendorOld: 'crates/_vendor/colord-2.9.1/',
        bunCacheNew: 'node_modules/.bun/colord@2.9.3/node_modules/colord/',
        rustCrate: 'crates/colord/',
        sourceSubdir: '',
        ownerFiles: [
            'index.mjs', 'colord.js', 'helpers.js', 'parse.js',
            'random.js', 'constants.js',
            'plugins/names.js',
            'plugins/a11y.js', 'plugins/harmonies.js', 'plugins/hwb.js',
            'plugins/lab.js', 'plugins/minify.js', 'plugins/mix.js',
        ],
        knownDeltas: [
            'Two patch versions of color-math changes. Diff `parse.js` and `colord.js` carefully (HSL/RGB rounding, alpha format, `#fff` vs `#ffffff` short-form decisions).',
            'Used by `postcss-colormin` (color minification — the highest-risk cssnano plugin) and `postcss-minify-gradients`.',
        ],
        verifyGates: [
            // colord-aware cssnano plugins (postcss-colormin, postcss-minify-gradients)
            // are still scaffolded — no parity gates exist yet. Verification
            // is via crates/colord/ unit tests + adversarial color corpus.
            'postcss-core-roundtrip',
        ],
        reportPath: 'crates/_vendor/COLORD_2.9.1_TO_2.9.3_AUDIT.md',
        notes: '`crates/colord/` is currently SCAFFOLDED — the port itself hasn\'t been written. This audit can flow into the initial port: target 2.9.3 source from the start, do not port 2.9.1 first. Mark all upstream-2.9.1-equivalent code paths with comments noting the 2.9.3 deltas.',
    },

    // ----- Tier 3: pinned at AFM-correct version, no drift, re-audit only -----
    // For these the audit is "verify Rust port matches AFM-pinned source
    // line-by-line." The risk is "we ported imperfectly to begin with,"
    // not "version changed under us."

    'postcss-nested': {
        kind: 'no-drift-reaudit',
        version: '5.0.6',
        bunCache: 'node_modules/.bun/postcss-nested@5.0.6/node_modules/postcss-nested/',
        rustCrate: 'crates/postcss-nested/',
        sourceSubdir: '',
        ownerFiles: ['index.js'],
        verifyGates: ['postcss-nested', 'postcss-core-roundtrip'],
        reportPath: 'crates/_vendor/POSTCSS_NESTED_5.0.6_REAUDIT.md',
        notes: '`bubble: ["starting-style", ...]` and `unwrap: [...]` config in `transform.ts:48-61` is part of the call site; the plugin\'s INTERPRETATION of those options must match v5 exactly. v5 → v6 changed selector merging semantics — do not consult v6 source.',
    },

    'postcss-normalize-whitespace': {
        kind: 'no-drift-reaudit',
        version: '5.1.1',
        bunCache: 'node_modules/.bun/postcss-normalize-whitespace@5.1.1/node_modules/postcss-normalize-whitespace/',
        rustCrate: 'crates/postcss-normalize-whitespace/',
        sourceSubdir: 'src/',
        ownerFiles: ['index.js'],
        verifyGates: ['postcss-normalize-whitespace', 'postcss-core-roundtrip'],
        reportPath: 'crates/_vendor/POSTCSS_NORMALIZE_WHITESPACE_5.1.1_REAUDIT.md',
        notes: 'Small plugin. Every space matters here, doubly — the whole point of the plugin is whitespace normalization.',
    },

    'postcss-discard-duplicates': {
        kind: 'no-drift-reaudit',
        version: '6.0.0',
        bunCache: 'node_modules/.bun/postcss-discard-duplicates@6.0.0/node_modules/postcss-discard-duplicates/',
        rustCrate: 'crates/postcss-discard-duplicates/',
        sourceSubdir: 'src/',
        ownerFiles: ['index.js'],
        verifyGates: ['npm-postcss-discard-duplicates', 'sort'],
        reportPath: 'crates/_vendor/POSTCSS_DISCARD_DUPLICATES_6.0.0_REAUDIT.md',
        notes: 'Distinct from the LOCAL `discard-duplicates` plugin in `crates/compiled-css/src/plugins/discard_duplicates.rs`. This is the npm v6 used by `sort.ts`. Different code, different ports.',
    },

    'postcss-values-parser': {
        kind: 'no-drift-reaudit',
        version: '6.0.2',
        bunCache: 'node_modules/.bun/postcss-values-parser@6.0.2/node_modules/postcss-values-parser/',
        rustCrate: 'crates/postcss-values-parser/',
        sourceSubdir: '',
        ownerFiles: ['lib/'],
        verifyGates: ['expand-shorthands', 'postcss-core-roundtrip'],
        reportPath: 'crates/_vendor/POSTCSS_VALUES_PARSER_6.0.2_REAUDIT.md',
        notes: 'PLURAL — distinct from `postcss-value-parser` (singular, 4.2.0). Different AST node types: `Numeric`, `Word`, `Func`. Used exclusively by `packages/css/src/plugins/expand-shorthands/*.ts`. Confirm round-trip identity over a value corpus.',
    },

    'postcss-value-parser': {
        kind: 'no-drift-reaudit',
        version: '4.2.0',
        bunCache: 'node_modules/.bun/postcss-value-parser@4.2.0/node_modules/postcss-value-parser/',
        rustCrate: 'crates/postcss-value-parser/',
        sourceSubdir: 'lib/',
        ownerFiles: ['index.js', 'parse.js', 'stringify.js', 'walk.js', 'unit.js'],
        verifyGates: ['postcss-core-roundtrip'],
        reportPath: 'crates/_vendor/POSTCSS_VALUE_PARSER_4.2.0_REAUDIT.md',
        notes: 'SINGULAR — distinct from `postcss-values-parser` (plural, 6.0.2). Used by `autoprefixer` and several cssnano plugins. Confirm round-trip identity over a value corpus.',
    },

    'cssnano-utils': {
        kind: 'no-drift-reaudit',
        version: '3.1.0',
        bunCache: 'node_modules/.bun/cssnano-utils@3.1.0/node_modules/cssnano-utils/',
        rustCrate: 'crates/cssnano-utils/',
        sourceSubdir: 'src/',
        ownerFiles: ['*.js'],
        verifyGates: ['postcss-core-roundtrip'],
        reportPath: 'crates/_vendor/CSSNANO_UTILS_3.1.0_REAUDIT.md',
        notes: 'Shared helpers used by ~every cssnano plugin we run. A bug here propagates everywhere.',
    },

    'caniuse-api': {
        kind: 'no-drift-reaudit',
        version: '3.0.0',
        bunCache: 'node_modules/.bun/caniuse-api@3.0.0/node_modules/caniuse-api/',
        rustCrate: 'crates/caniuse-api/',
        sourceSubdir: 'src/',
        ownerFiles: ['*.js'],
        verifyGates: ['postcss-core-roundtrip'],
        reportPath: 'crates/_vendor/CANIUSE_API_3.0.0_REAUDIT.md',
        notes: 'Wrapper used by `postcss-colormin`, `postcss-minify-params`, `postcss-reduce-initial`, `postcss-convert-values`, `postcss-normalize-unicode` to query caniuse-lite via browserslist targets. The query-against-targets API surface must match exactly.',
    },

    'fraction.js': {
        kind: 'no-drift-reaudit',
        version: '4.2.0',
        bunCache: 'node_modules/.bun/fraction.js@4.2.0/node_modules/fraction.js/',
        rustCrate: 'crates/fraction-js/',
        sourceSubdir: '',
        ownerFiles: ['fraction.js'],
        verifyGates: ['postcss-core-roundtrip'],
        reportPath: 'crates/_vendor/FRACTION_JS_4.2.0_REAUDIT.md',
        notes: 'Used in autoprefixer\'s grid math AND `postcss-convert-values`. Output of these depends on stringified fraction outputs — byte parity matters. Audit `add`, `sub`, `mul`, `div`, `toString`, `toFraction` against test vectors.',
    },

    'postcss-discard-comments': {
        kind: 'no-drift-reaudit',
        version: '5.1.2',
        bunCache: 'node_modules/.bun/postcss-discard-comments@5.1.2/node_modules/postcss-discard-comments/',
        rustCrate: 'crates/cssnano-postcss-discard-comments/',
        sourceSubdir: 'src/',
        ownerFiles: ['*.js'],
        verifyGates: ['postcss-discard-comments'],
        reportPath: 'crates/_vendor/POSTCSS_DISCARD_COMMENTS_5.1.2_REAUDIT.md',
        notes: 'Default keeps `/*!` important comments, drops the rest. Confirm the default removal predicate matches.',
    },

    'postcss-normalize-string': {
        kind: 'no-drift-reaudit',
        version: '5.1.0',
        bunCache: 'node_modules/.bun/postcss-normalize-string@5.1.0/node_modules/postcss-normalize-string/',
        rustCrate: 'crates/cssnano-postcss-normalize-string/',
        sourceSubdir: 'src/',
        ownerFiles: ['*.js'],
        verifyGates: ['postcss-normalize-string'],
        reportPath: 'crates/_vendor/POSTCSS_NORMALIZE_STRING_5.1.0_REAUDIT.md',
        notes: 'Default `preferredQuote: "double"`. Touches every quoted string value.',
    },

    'postcss-normalize-positions': {
        kind: 'no-drift-reaudit',
        version: '5.1.1',
        bunCache: 'node_modules/.bun/postcss-normalize-positions@5.1.1/node_modules/postcss-normalize-positions/',
        rustCrate: 'crates/cssnano-postcss-normalize-positions/',
        sourceSubdir: 'src/',
        ownerFiles: ['*.js'],
        verifyGates: ['postcss-normalize-positions'],
        reportPath: 'crates/_vendor/POSTCSS_NORMALIZE_POSITIONS_5.1.1_REAUDIT.md',
        notes: 'Rewrites `background-position` and `*-perspective-origin` keyword pairs (left/top → 0 0, etc.). No options.',
    },

    'postcss-normalize-timing-functions': {
        kind: 'no-drift-reaudit',
        version: '5.1.0',
        bunCache: 'node_modules/.bun/postcss-normalize-timing-functions@5.1.0/node_modules/postcss-normalize-timing-functions/',
        rustCrate: 'crates/cssnano-postcss-normalize-timing-functions/',
        sourceSubdir: 'src/',
        ownerFiles: ['*.js'],
        verifyGates: ['postcss-normalize-timing-functions'],
        reportPath: 'crates/_vendor/POSTCSS_NORMALIZE_TIMING_FUNCTIONS_5.1.0_REAUDIT.md',
        notes: 'Compresses `cubic-bezier(...)` / `steps(...)` to keyword equivalents (ease/linear/etc), strips redundant trailing `, end` from `steps(N, end)`. No options.',
    },

    'postcss-normalize-url': {
        kind: 'no-drift-reaudit',
        version: '5.1.0',
        bunCache: 'node_modules/.bun/postcss-normalize-url@5.1.0/node_modules/postcss-normalize-url/',
        rustCrate: 'crates/cssnano-postcss-normalize-url/',
        sourceSubdir: 'src/',
        ownerFiles: ['*.js'],
        verifyGates: ['postcss-normalize-url'],
        reportPath: 'crates/_vendor/POSTCSS_NORMALIZE_URL_5.1.0_REAUDIT.md',
        notes: 'Walks every Decl value and `@namespace` AtRule params; rewrites the inner of `url(...)` calls. Absolute/protocol-relative URLs pass through `normalize-url@6.1.0`. Relative paths pass through `path.posix.normalize`. The 5 postcss-side overrides hold (`normalizeProtocol`/`sortQueryParameters`/`stripHash`/`stripWWW`/`stripTextFragment` all `false`).',
    },

    // ----- Tier 4: local plugins from packages/css/src/plugins/ -----

    'compiled-css-local-plugins': {
        kind: 'local-plugins',
        afmCommit: '40a45489eaaacc023110c3f107d702a389232892',
        afmCommitShort: '40a4548',
        sourceTreeNow: 'packages/css/src/plugins/',
        rustCrate: 'crates/compiled-css/src/plugins/',
        verifyGates: [
            'discard-empty-rules', 'discard-duplicates', 'extract-stylesheets',
            'parent-orphaned-pseudos', 'increase-specificity',
            'merge-duplicate-at-rules', 'normalize-current-color',
            'sort-atomic-style-sheet', 'atomicify-rules', 'expand-shorthands',
            'sort',
        ],
        reportPath: 'crates/_vendor/COMPILED_CSS_LOCAL_PLUGINS_AFM_REAUDIT.md',
        notes: '`packages/css/src/` was overlaid with `@compiled/css@0.19.0` source from commit 40a4548 during the AFM repin. Three files were known to revert (sort-atomic-style-sheet, expand-shorthands/flex, parse-at-rule rename) and the corresponding Rust ports were patched. This audit does a fresh diff between the AFM commit\'s plugins/ tree and our current Rust ports to catch ANY other drift that was missed during the overlay.',
    },
};

// ---------------------------------------------------------------------------
// Prompt templates.
// ---------------------------------------------------------------------------

function header(title) {
    return `# ${title}\n`;
}

function backgroundBoilerplate() {
    return `
## Background — why this exists

The Rust ports under \`crates/\` were originally written against the
versions pinned in \`REFERENCE_LOCK_FILE/yarn.lock\` (the upstream
\`compiled\` repo's lockfile). We later discovered that the **AFM/JIRA
monorepo** — the actual consumer of the Rust port — installs
\`@compiled/css@0.19.0\` resolved against a different dependency graph.
AFM resolution wins; see \`AFM_MONOREPO_DEPENDENCIES_MORE.md\` and
\`crates/PARITY_VERSIONS.md\` "Source of Truth" section.

The contract for this project is **byte-equality** for hash output.
Any non-cosmetic source change between the two pinned versions must
be replicated in the Rust port. The 20-stage parity corpus in
\`crates/parity-runner/corpus/\` is a SMOKE gate (~430 hand-crafted
inputs total); the real consumer is **~60GB of AFM source** where
every selector/value/at-rule edge case will surface. **Do not assume
a change is cosmetic just because the existing corpus passes.** Bias
toward replicating upstream verbatim. The cost of a needless port is
low; the cost of a missed semantic change is a silent hash divergence
in production that is effectively impossible to debug.
`;
}

function constraintsBoilerplate() {
    return `
## Constraints (do NOT break these)

- **Do not modify** \`packages/css/src/\`. That tree is the JS oracle
  pinned at \`@compiled/css@0.19.0\` (commit 40a4548) — touching it
  invalidates parity for every other agent.
- **Do not modify** the "Pinned Versions" tables in
  \`crates/PARITY_VERSIONS.md\`. The pin is already correct. You're
  closing the gap between the pin and the implementation, not changing
  the pin itself.
- **Do not modify** any \`crates/_vendor/<pkg>-<old-version>/\` directory.
  Those are read-only historical references.
- **Do not delete** any existing corpus entry, even if it looks
  redundant. Only add new ones.
- **Do not bypass** \`RUSTFLAGS=""\` by adjusting workspace
  \`Cargo.toml\` or \`compiled-css-napi\`'s \`[profile.release]\`.
  The clearing is the correct workaround for the proc-macro/LTO conflict.
- **Do not skip** any verification gate. Even gates that look unrelated
  to your package may exercise it transitively (e.g. \`postcss-core-roundtrip\`
  depends on every postcss-touching plugin's AST shape).
- If you find a delta that's ambiguous — could be cosmetic, could be
  semantic — **port it**. Bias toward replicating upstream verbatim.
- **Do not "improve" anything along the way.** Bugs are features. If the
  newer version has a regression vs the older one, port the regression.
- **HashMap is banned** in any code path that produces output bytes.
  Use \`IndexMap\` (insertion-order is byte-affecting downstream).
- **Do not run \`bun install\`** unless you genuinely need to refresh
  \`node_modules\`. The AFM-pinned versions are already resolved; an
  unprompted \`bun install\` can churn the lockfile and confuse other
  concurrent agents.
`;
}

function verificationGatesBlock(gates) {
    const lines = gates.map((g) =>
        `crates/target/debug/parity-runner --stage ${g} --corpus crates/parity-runner/corpus/${g}`
    ).join('\n');
    return `
## Verification gates (must all pass before declaring done)

Run each command from the workspace root. If any fails, that's a
regression — investigate before declaring complete.

\`\`\`bash
# Build the parity-runner if it isn't already built.
RUSTFLAGS="" cargo build --manifest-path crates/parity-runner/Cargo.toml

# Full Rust test suite — must stay green.
RUSTFLAGS="" cargo test --manifest-path crates/Cargo.toml --workspace --no-fail-fast

# Parity gates — ALL must remain byte-clean (JS-vs-Rust).
${lines}

# NAPI sort + engine flag verifiers — must stay 12/12.
bun run packages/css/scripts/verify-napi-sort.mjs
bun run packages/css/scripts/verify-engine-flag.mjs

# Determinism on at least one stage you touched (JS-vs-JS oracle stability).
${gates[0] ? `crates/target/debug/parity-runner --stage ${gates[0]} --corpus crates/parity-runner/corpus/${gates[0]} --determinism` : '# (no parity stage exercises your package directly — rely on unit tests)'}
\`\`\`

If \`cargo build\` complains about \`lto cannot be used for proc-macro\`,
prefix the command with \`RUSTFLAGS=""\`. The repo's user-level
RUSTFLAGS conflicts with proc-macro builds — clearing it is the standard
workaround.
`;
}

function reportBlock(reportPath) {
    return `
## Report

Write a concise audit document at \`${reportPath}\` containing:

- A table of every file in the package source with a column for
  "cosmetic / non-cosmetic / no diff" and a one-line explanation per
  non-cosmetic entry.
- The list of Rust files you modified and a one-line description of
  what changed in each.
- The corpus entries you added and which code path each exercises.
- Verification gate results (paste the actual final-line output of
  each command in the verification block).

Update \`crates/STATUS.md\` "AFM repin" section: append a single
sub-section recording the change. **Do not** touch other STATUS sections.
`;
}

// ---------------------------------------------------------------------------
// Per-kind templates.
// ---------------------------------------------------------------------------

function versionDriftPrompt(name, cfg) {
    const knownDeltasList = cfg.knownDeltas.map((d) => `- ${d}`).join('\n');
    const ownerFilesList = cfg.ownerFiles.map((f) => `\`${f}\``).join(', ');
    const subdirHint = cfg.sourceSubdir ? `\`${cfg.sourceSubdir}\` subdirectory of` : 'root of';
    const vendorOldLine = cfg.vendorOld
        ? `- **Old (${cfg.was})**: \`${cfg.vendorOld}\``
        : `- **Old (${cfg.was})**: NOT vendored. To obtain: \`npm pack ${name}@${cfg.was}\` and extract, OR check \`node_modules/.bun/${name}@${cfg.was}/\` for a stale leftover from before the repin.`;

    // Banner for data-only / not-yet-consumed packages — these don't need
    // the full source-port treatment because the upstream is data tables
    // (not JS source) and/or no Rust code consumes them yet.
    const consumedByLine = cfg.consumedBy
        ? `\n### Where this is consumed in the Rust port\n\n${cfg.consumedBy}\n`
        : '';
    const banners = [];
    if (cfg.dataOnly) {
        banners.push(`> **DATA-ONLY PACKAGE.** This is a JSON/data dependency, not JS source. There is no \`<file>.js\` → \`<file>.rs\` mapping. The audit verifies the **vendored data tables** match AFM's installed snapshot, NOT that any Rust source matches an upstream JS file.`);
    }
    if (cfg.notYetConsumed) {
        banners.push(`> **NOT YET CONSUMED BY ANY RUST CODE.** \`grep -rn "${name}\\|${name.replace(/-/g, '_')}" crates/\` returns hits only in \`Cargo.toml\` description strings, not in any \`*.rs\` source. The Rust port has nothing to update today; this audit is a documentation/forward-pin sanity check. **Consider deferring this prompt entirely** unless a downstream port (autoprefixer, browserslist-aware cssnano plugins) is about to land that will consume it. If you proceed, the deliverable shrinks to (a) re-vendor under \`crates/_vendor/${name}-${cfg.now}/\`, (b) bump pin docstrings, (c) write a one-page report, (d) flag the hand-off in STATUS.md. Skip the source diff. Skip the Rust port. Skip the corpus additions.`);
    }
    const bannerBlock = banners.length ? '\n' + banners.join('\n\n') + '\n' : '';

    // Tasks block — different shape for data-only vs source-port.
    const tasksBlock = cfg.dataOnly
        ? `## Your task

### 1. Confirm the new vendored snapshot reflects the AFM pin

\`\`\`bash
diff -r \\
  ${cfg.vendorOld || '<old-source-path>'}${cfg.sourceSubdir} \\
  ${cfg.bunCacheNew}${cfg.sourceSubdir}
\`\`\`

Walk every file in the diff. Categorize each delta:

- **Data-only changes** (new browser version added, support flag flipped,
  feature added/removed) — record in your report. These reach output
  bytes ONLY through downstream consumers (autoprefixer, caniuse-api,
  the browserslist-aware cssnano plugins).
- **Schema changes** (field added/removed at the JSON level) — these
  break the unpacker / parser. Update \`crates/caniuse-db/scripts/snapshot.js\`
  and \`crates/caniuse-db/src/features.rs\` / \`agents.rs\` if hit.

### 2. Re-run the snapshot regeneration if needed

\`\`\`bash
node crates/caniuse-db/scripts/snapshot.js
RUSTFLAGS="" cargo build --manifest-path crates/caniuse-db/Cargo.toml
\`\`\`

The snapshot file (\`crates/caniuse-db/data/features.snapshot.json\`)
has already been regenerated as part of the AFM repin. Verify that file
contains the new version string at the head and the expected feature
count. Do NOT re-vendor on top of work already done.

### 3. Spot-check downstream consumers

For caniuse-lite specifically: pick 5–10 high-traffic features (flexbox,
grid, position-sticky, mask, aspect-ratio, container queries, :has,
transforms, gradients, css-variables) and confirm Rust-side
\`caniuse_db::feature("X")\` returns the same support matrix as
Node-side \`require("caniuse-lite/data/features/X.js")\` post-unpack.
Add unit tests under \`crates/caniuse-db/src/lib.rs\` or
\`crates/caniuse-api/src/lib.rs\` for any spot-check that surfaces
unexpected drift.

For electron-to-chromium / node-releases: there is no current Rust
consumer. Skip this step. Your report's "future hand-off" section
substitutes for it.${cfg.notYetConsumed ? `

### 4. (skip if not-yet-consumed)

For \`${name}\`, this step does not apply — no Rust code reads the data.` : ''}`
        : `## Your task

### 1. Full source-tree diff

Run \`diff -r\` on the entire ${cfg.sourceSubdir ? `\`${cfg.sourceSubdir}\`` : 'package'} tree between the two versions.
Categorize every change:

- **Cosmetic** (whitespace, comment edits, variable renames with no
  semantic effect) — list but do not port.
- **Non-cosmetic** (control flow, output, AST shape, regex, sort, raws
  handling, default options, error messages) — port into the Rust crate.

Starting command:

\`\`\`bash
diff -r \\
  ${cfg.vendorOld || '<old-source-path>'} \\
  ${cfg.bunCacheNew}
\`\`\`

Walk **every file** under ${cfg.sourceSubdir ? `\`${cfg.sourceSubdir}\`` : 'the package'}.
Don't stop at the first file. Don't trust headline files only.

### 2. Port every non-cosmetic delta into \`${cfg.rustCrate}\`

For each non-cosmetic change you identify:

- Locate the corresponding Rust file (mapping is 1:1 by filename
  — \`parser.js\` → \`parser.rs\`, etc.).
- Apply the equivalent change in Rust.
- Add a **brief** comment citing the upstream change:
  \`// ${cfg.now}: <one-line summary> (file.js line ~N)\`. Do not write
  paragraphs.
- Hashmap → \`IndexMap\` always.

### 3. Add adversarial corpus entries

The current corpus does not necessarily cover the changed code paths.
Add files to the parity-runner corpora for the verification stages
listed below. Every changed code path needs at least one input that
exercises it. File naming: \`corpus/<stage>/NN_<short_label>.css\`.
Pick \`NN\` numbers that don't collide with existing entries.`;

    return `${header(`Audit & Port: \`${name}\` ${cfg.was} → ${cfg.now}`)}${bannerBlock}
${backgroundBoilerplate()}

## Specific to \`${name}\`

We originally ported the Rust crate \`${cfg.rustCrate}\` against
**${cfg.was}**. AFM resolves to **${cfg.now}**. The pin has been bumped
in \`PARITY_VERSIONS.md\`, root \`package.json\` overrides, and the
crate's docstrings — **but the port itself has not been audited against
${cfg.now} source yet**.

${cfg.notes}
${consumedByLine}
### Known non-cosmetic deltas (anchors — find more in your full diff)

${knownDeltasList}

## Source locations

${vendorOldLine}
- **New (${cfg.now})**: \`${cfg.bunCacheNew}\`
- **Rust port**: \`${cfg.rustCrate}\`
- **Headline files** (in the ${subdirHint} the package): ${ownerFilesList}

If \`node_modules/.bun/${name}@${cfg.now}*/\` is missing, run
\`bun install\` from the workspace root first. If you want a vendored
copy for permanence:
\`mkdir -p crates/_vendor/${name}-${cfg.now}/package && cp -r ${cfg.bunCacheNew}. crates/_vendor/${name}-${cfg.now}/package/\`.

${tasksBlock}

${verificationGatesBlock(cfg.verifyGates)}
${reportBlock(cfg.reportPath)}
${constraintsBoilerplate()}
`;
}

function noDriftReauditPrompt(name, cfg) {
    const ownerFilesList = cfg.ownerFiles.map((f) => `\`${f}\``).join(', ');
    const subdirHint = cfg.sourceSubdir ? `\`${cfg.sourceSubdir}\` subdirectory of` : 'root of';

    return `${header(`Re-audit: \`${name}@${cfg.version}\` (no drift)`)}
${backgroundBoilerplate()}

## Specific to \`${name}\`

\`${name}@${cfg.version}\` is **NOT** drifted between the
REFERENCE_LOCK_FILE and AFM resolution — both pin the same version.
This audit exists because the original port may have introduced
mistakes that the existing 20-stage parity corpus doesn't catch.
The risk model: "we ported imperfectly to begin with," not "version
changed under us."

${cfg.notes}

## Source locations

- **AFM-pinned source (${cfg.version})**: \`${cfg.bunCache}\`
- **Rust port**: \`${cfg.rustCrate}\`
- **Headline files** (in the ${subdirHint} the package): ${ownerFilesList}

## Your task

### 1. Full source-tree walk

Walk every file in ${cfg.sourceSubdir ? `\`${cfg.bunCache}${cfg.sourceSubdir}\`` : `\`${cfg.bunCache}\``}.
For each file, locate the corresponding Rust port and verify line-by-line
that:

- Every control-flow branch matches.
- Every regex matches (audit Unicode classes — JS regex semantics differ
  from Rust's \`regex\` crate in subtle places).
- Every sort comparator matches, including tie-break ordering. Rust's
  \`sort_by\` is stable (matches JS since ES2019), but the comparator
  must produce identical orderings even for "equal" elements.
- Every default option value matches.
- Every numeric stringification matches. JS's \`String(0.1+0.2)\` =
  \`"0.30000000000000004"\`; Rust's \`format!("{}", ...)\` may not agree
  on edge cases. Use a JS-double-to-string algorithm where any output
  path stringifies a number.
- Every iteration order matches. Banned: \`HashMap\` in output paths.
- Every raws field is preserved 1:1.

### 2. Apply fixes for any divergence found

For each fix:
- Cite the upstream file + line in a brief comment.
- Add a regression test under \`#[cfg(test)] mod tests\`.
- Bias toward replicating upstream verbatim — do not "improve."

### 3. Add adversarial corpus entries for any code path you touched

Same approach as the version-drift template. Files go to the
parity-runner corpora for the verification stages below.

${verificationGatesBlock(cfg.verifyGates)}
${reportBlock(cfg.reportPath)}
${constraintsBoilerplate()}
`;
}

function localPluginsPrompt(name, cfg) {
    return `${header(`Re-audit: \`compiled-css\` local plugins vs @compiled/css@0.19.0 (${cfg.afmCommitShort})`)}
${backgroundBoilerplate()}

## Specific to local plugins

\`packages/css/src/plugins/\` was OVERLAID during the AFM repin with
the source tree from upstream commit \`${cfg.afmCommit}\` (i.e.
\`@compiled/css@0.19.0\`). The overlay reverted three files (the
\`flatten-multiple-selectors\` deletion, the \`expand-shorthands/flex.ts\`
simplification, and the \`parse-at-rule.ts\` rename) and the corresponding
Rust ports in \`${cfg.rustCrate}\` were patched.

This audit does a **fresh diff between the AFM commit's plugins/ tree
and our current Rust ports**, to catch any other drift the overlay
missed.

${cfg.notes}

## Source locations

- **AFM-pinned JS source**: \`${cfg.sourceTreeNow}\` (this directory IS the
  AFM commit\\'s plugins/ tree — already overlaid).
- **Reference checkout** (read-only): \`/c/Users/shanon/Documents/projects/compiled\`
  at commit \`${cfg.afmCommit}\`. Use
  \`git -C /c/Users/shanon/Documents/projects/compiled show ${cfg.afmCommitShort}:packages/css/src/plugins/<file>\`
  to fetch the canonical source if you suspect \`${cfg.sourceTreeNow}\`
  was edited.
- **Rust port**: \`${cfg.rustCrate}\`

## Your task

### 1. Verify the JS oracle hasn't drifted

Confirm \`${cfg.sourceTreeNow}\` is byte-identical to the AFM commit's
\`packages/css/src/plugins/\` tree:

\`\`\`bash
diff -r \\
  <(git -C /c/Users/shanon/Documents/projects/compiled archive ${cfg.afmCommitShort} packages/css/src/plugins | tar -t) \\
  <(find ${cfg.sourceTreeNow} -type f | sort | sed 's|^${cfg.sourceTreeNow}|packages/css/src/plugins/|')
\`\`\`

If files differ, flag them — someone may have re-edited the JS oracle.
DO NOT fix them yourself; surface the drift in your report.

### 2. Walk every plugin file

For each file under \`${cfg.sourceTreeNow}\`, locate the corresponding
Rust port and verify line-by-line as in the no-drift template
(control flow, regex, sort comparators, default options, numeric
stringification, iteration order, raws preservation).

The mapping is 1:1 by filename:
- \`atomicify-rules.ts\` → \`atomicify_rules.rs\`
- \`discard-duplicates.ts\` → \`discard_duplicates.rs\`
- \`discard-empty-rules.ts\` → \`discard_empty_rules.rs\`
- \`expand-shorthands/<file>.ts\` → \`expand_shorthands/<file>.rs\`
- \`extract-stylesheets.ts\` → \`extract_stylesheets.rs\`
- \`increase-specificity.ts\` → \`increase_specificity.rs\`
- \`merge-duplicate-at-rules.ts\` → \`merge_duplicate_at_rules.rs\`
- \`normalize-css.ts\` → \`normalize_css.rs\`
- \`normalize-current-color.ts\` → \`normalize_current_color.rs\`
- \`parent-orphaned-pseudos.ts\` → \`parent_orphaned_pseudos.rs\`
- \`sort-atomic-style-sheet.ts\` → \`sort_atomic_style_sheet.rs\`
- \`sort-shorthand-declarations.ts\` → \`sort_shorthand_declarations.rs\`
- \`at-rules/<file>.ts\` → \`at_rules/<file>.rs\`

### 3. Pay special attention to:

- **\`atomicify-rules.ts\`**: the CRITICAL hash plugin. Class-name hash
  output reaches every consumer — bit-identical hashing is a hard
  invariant. Re-verify the hash function port (\`crates/sjcompiled-utils\`)
  too.
- **\`sort-atomic-style-sheet.ts\`**: was reverted during the AFM repin
  (now uses \`parseAtRule\` not \`parseMediaQuery\`, no name=="media"
  gate). Confirm the Rust matches.
- **\`expand-shorthands/flex.ts\`**: was reverted during the AFM repin
  (only handles \`none\` keyword, drops \`auto\`/\`initial\`/\`revert\`/etc.
  branches that were added in 0.20+). Confirm the Rust matches.
- **\`normalize-css.ts\`**: the BASE_PLUGINS / PROD_PLUGINS filter list.
  Plugin set is 14 + normalizeCurrentColor. Confirm cssnano-preset-default
  source order is preserved (NOT the order in normalize-css.ts arrays —
  Anomaly #7 in PARITY_VERSIONS.md).

${verificationGatesBlock(cfg.verifyGates)}
${reportBlock(cfg.reportPath)}
${constraintsBoilerplate()}
`;
}

// ---------------------------------------------------------------------------
// Dispatch.
// ---------------------------------------------------------------------------

function generatePrompt(name) {
    const cfg = PACKAGES[name];
    if (!cfg) throw new Error(`unknown package: ${name}`);
    switch (cfg.kind) {
        case 'version-drift':    return versionDriftPrompt(name, cfg);
        case 'no-drift-reaudit': return noDriftReauditPrompt(name, cfg);
        case 'local-plugins':    return localPluginsPrompt(name, cfg);
        default: throw new Error(`unknown kind for ${name}: ${cfg.kind}`);
    }
}

function usage() {
    const names = Object.keys(PACKAGES).join('\n  ');
    process.stderr.write(`Usage:
  node scripts/gen-audit-prompt.mjs <package-name>
  node scripts/gen-audit-prompt.mjs --list
  node scripts/gen-audit-prompt.mjs --check                   # verify every
                                                              # rustCrate /
                                                              # vendorOld path
                                                              # exists on disk
  node scripts/gen-audit-prompt.mjs --all                     # writes one .md per
                                                              # package into
                                                              # ./scripts/audit-prompts/
  node scripts/gen-audit-prompt.mjs --all --stdout            # dump to stdout instead
  node scripts/gen-audit-prompt.mjs <package-name> --out <path>

Known packages:
  ${names}
`);
}

function fileSafeName(name) {
    // Package names can contain `.` (e.g. fraction.js) and `/` (scoped, none
    // here today). Replace `/` with `__` so the filename stays one-segment.
    // Dots are fine on every modern filesystem.
    return name.replace(/\//g, '__');
}

// Verify every package's `rustCrate` path actually exists relative to the
// repo root. Catches drift between this registry and the workspace.
// Returns an array of `{ name, path, problem }` entries for any mismatch.
function checkRegistry() {
    const problems = [];
    for (const [name, cfg] of Object.entries(PACKAGES)) {
        const rustCrate = cfg.rustCrate;
        if (!rustCrate) {
            problems.push({ name, path: '(none)', problem: 'rustCrate field missing' });
            continue;
        }
        const abs = resolve(REPO_ROOT, rustCrate);
        if (!existsSync(abs)) {
            problems.push({ name, path: rustCrate, problem: 'rustCrate directory does not exist' });
        }
        if (cfg.kind === 'version-drift' && cfg.vendorOld) {
            const vendorAbs = resolve(REPO_ROOT, cfg.vendorOld);
            if (!existsSync(vendorAbs)) {
                problems.push({ name, path: cfg.vendorOld, problem: 'vendorOld path does not exist (the prompt will instruct the agent to fetch it)' });
            }
        }
    }
    return problems;
}

function main() {
    const args = process.argv.slice(2);
    if (args.length === 0 || args.includes('--help') || args.includes('-h')) {
        usage();
        process.exit(args.length === 0 ? 2 : 0);
    }
    if (args[0] === '--check') {
        const problems = checkRegistry();
        if (problems.length === 0) {
            process.stderr.write('OK — every rustCrate path in the registry exists.\n');
            process.exit(0);
        }
        process.stderr.write(`registry has ${problems.length} issue(s):\n`);
        for (const { name, path, problem } of problems) {
            process.stderr.write(`  ${name.padEnd(40)} ${path.padEnd(60)} ${problem}\n`);
        }
        process.exit(1);
    }
    if (args[0] === '--list') {
        for (const name of Object.keys(PACKAGES)) {
            const cfg = PACKAGES[name];
            const tag = cfg.kind === 'version-drift'
                ? `${cfg.was} → ${cfg.now}`
                : cfg.kind === 'no-drift-reaudit'
                    ? `@${cfg.version} (no drift, re-audit)`
                    : `(local plugins re-audit)`;
            process.stdout.write(`${name.padEnd(40)}  ${tag}\n`);
        }
        process.exit(0);
    }
    if (args[0] === '--all') {
        // Self-check before writing — if a path doesn't exist, the prompt
        // will misdirect the agent. Surface that loudly.
        const problems = checkRegistry();
        if (problems.length > 0) {
            process.stderr.write(`WARNING — registry has ${problems.length} issue(s); prompts may misdirect agents:\n`);
            for (const { name, path, problem } of problems) {
                process.stderr.write(`  ${name.padEnd(40)} ${path.padEnd(60)} ${problem}\n`);
            }
            process.stderr.write('\n');
        }
        const toStdout = args.includes('--stdout');
        if (toStdout) {
            for (const name of Object.keys(PACKAGES)) {
                process.stdout.write(generatePrompt(name));
                process.stdout.write('\n\n---\n\n');
            }
            process.exit(0);
        }
        const outDir = resolve(SCRIPT_DIR, 'audit-prompts');
        mkdirSync(outDir, { recursive: true });
        const written = [];
        for (const name of Object.keys(PACKAGES)) {
            const file = resolve(outDir, `audit-${fileSafeName(name)}.md`);
            const prompt = generatePrompt(name);
            writeFileSync(file, prompt);
            written.push({ file, bytes: prompt.length });
        }
        process.stderr.write(`wrote ${written.length} prompts to ${outDir}\n`);
        for (const { file, bytes } of written) {
            process.stderr.write(`  ${file}  (${bytes} bytes)\n`);
        }
        process.exit(0);
        // unreachable; keep the original loop below from running
        // eslint-disable-next-line no-unreachable
        for (const _ of []) {
            process.stdout.write(generatePrompt(_));
            process.stdout.write('\n\n---\n\n');
        }
        process.exit(0);
    }
    const name = args[0];
    if (!PACKAGES[name]) {
        process.stderr.write(`unknown package: ${name}\n\n`);
        usage();
        process.exit(2);
    }
    const outIdx = args.indexOf('--out');
    const out = outIdx >= 0 ? args[outIdx + 1] : null;
    const prompt = generatePrompt(name);
    if (out) {
        writeFileSync(out, prompt);
        process.stderr.write(`wrote ${out} (${prompt.length} bytes)\n`);
    } else {
        process.stdout.write(prompt);
    }
}

main();
