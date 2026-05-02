import { createRequire } from 'node:module';

import postcss from 'postcss';
import discardDuplicates from 'postcss-discard-duplicates';

import { mergeDuplicateAtRules } from './plugins/merge-duplicate-at-rules';
import { sortAtomicStyleSheet } from './plugins/sort-atomic-style-sheet';

// Lazy-loaded Rust NAPI backend. Only resolved when
// `process.env.COMPILED_CSS_ENGINE === 'rust'` so consumers on
// platforms without a prebuilt binary aren't forced to depend on it
// at module-load time. The JS pipeline below remains the parity
// oracle and the default execution path for all other env values.
const requireFromHere = createRequire(import.meta.url);
let cachedNative: { sort: (css: string, opts: SortOpts | null) => string } | undefined;
function rustEngine(): { sort: (css: string, opts: SortOpts | null) => string } {
  if (!cachedNative) {
    cachedNative = requireFromHere('@sjcompiled/css-native');
  }
  return cachedNative!;
}

interface SortOpts {
  sortAtRulesEnabled: boolean | undefined;
  sortShorthandEnabled: boolean | undefined;
}

/**
 * Sorts an atomic style sheet.
 *
 * @param stylesheet
 * @returns the sorted stylesheet
 */
export function sort(
  stylesheet: string,
  {
    sortAtRulesEnabled,
    sortShorthandEnabled,
  }: { sortAtRulesEnabled: boolean | undefined; sortShorthandEnabled: boolean | undefined } = {
    // These default values should remain undefined so we don't override the default
    // values set in packages/css/src/plugins/sort-atomic-style-sheet.ts
    //
    // Modify packages/css/src/plugins/sort-atomic-style-sheet.ts if you want to
    // update the actual default values for sortAtRulesEnabled and sortShortEnabled.
    sortAtRulesEnabled: undefined,
    sortShorthandEnabled: undefined,
  }
): string {
  // Phase 8a: route through the Rust NAPI backend when explicitly
  // opted in. Output is byte-for-byte identical to the JS pipeline
  // below — verified by `crates/parity-runner` (Stage::Sort) and
  // `packages/css/scripts/verify-napi-sort.mjs`.
  if (process.env.COMPILED_CSS_ENGINE === 'rust') {
    return rustEngine().sort(stylesheet, { sortAtRulesEnabled, sortShorthandEnabled });
  }

  const result = postcss([
    discardDuplicates(),
    mergeDuplicateAtRules(),
    sortAtomicStyleSheet({ sortAtRulesEnabled, sortShorthandEnabled }),
  ]).process(stylesheet, {
    from: undefined,
  });

  return result.css;
}
