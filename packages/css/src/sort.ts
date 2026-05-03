import { createError } from '@compiled/utils';
import postcss from 'postcss';
import discardDuplicates from 'postcss-discard-duplicates';

import { mergeDuplicateAtRules } from './plugins/merge-duplicate-at-rules';
import { sortAtomicStyleSheet } from './plugins/sort-atomic-style-sheet';

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
  // Phase 8a NAPI engine flag (drift-fix landing — see
  // `crates/PHASE_8B_NAPI_NOTES.md` Drift §1). When
  // `COMPILED_CSS_ENGINE === 'rust'`, delegate to `@compiled/css-native`'s
  // synchronous Rust port. Mirrors the Phase 8b pattern in
  // `packages/css/src/transform.ts:32-82` verbatim — same env var, same
  // default behavior (default = JS), same `createError('css', 'Unhandled
  // exception')` envelope re-wrap on failure.
  //
  // The JS pipeline below the gate stays as the parity oracle and the
  // emergency fallback for the next 12+ months per EXECUTION_PLAN.md
  // Phase 10d — do NOT delete or restructure it.
  if (process.env.COMPILED_CSS_ENGINE === 'rust') {
    try {
      // eslint-disable-next-line @typescript-eslint/no-var-requires
      const { sort: rustSort } = require('@compiled/css-native');
      return rustSort(stylesheet, {
        sortAtRulesEnabled,
        sortShorthandEnabled,
      });
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : e;
      throw createError(
        'css',
        'Unhandled exception'
      )(
        `An unhandled exception was raised when parsing your CSS, this is probably a bug!
  Raise an issue here: https://github.com/atlassian-labs/compiled/issues/new?assignees=&labels=&template=bug_report.md&title=CSS%20Parsing%20Exception:%20

  Input CSS: {
    ${stylesheet}
  }

  Exception: ${message}`
      );
    }
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
