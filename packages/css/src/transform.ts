import { createError, unique } from '@sjcompiled/utils';
import autoprefixer from 'autoprefixer';
import postcss from 'postcss';
import nested from 'postcss-nested';
import whitespace from 'postcss-normalize-whitespace';

import { atomicifyRules } from './plugins/atomicify-rules';
import { discardDuplicates } from './plugins/discard-duplicates';
import { discardEmptyRules } from './plugins/discard-empty-rules';
import { expandShorthands } from './plugins/expand-shorthands';
import { extractStyleSheets } from './plugins/extract-stylesheets';
import { increaseSpecificity } from './plugins/increase-specificity';
import { normalizeCSS } from './plugins/normalize-css';
import { parentOrphanedPseudos } from './plugins/parent-orphaned-pseudos';
import { sortAtomicStyleSheet } from './plugins/sort-atomic-style-sheet';

export interface TransformOpts {
  optimizeCss?: boolean;
  classNameCompressionMap?: Record<string, string>;
  increaseSpecificity?: boolean;
  sortAtRules?: boolean;
  sortShorthand?: boolean;
  classHashPrefix?: string;
}

/**
 * Will transform CSS into multiple CSS sheets.
 *
 * @param css CSS string
 * @param opts Transformation options
 */
export const transformCss = (
  css: string,
  opts: TransformOpts
): { sheets: string[]; classNames: string[] } => {
  // Phase 8b NAPI engine flag. When `COMPILED_CSS_ENGINE === 'rust'`,
  // delegate to `@sjcompiled/css-native`'s synchronous Rust port. The
  // Rust port composes the same 12-plugin pipeline in postcss-lifecycle-
  // correct order (Once → walk → OnceExit) per
  // `crates/PHASE_8B_LIFECYCLE_AUDIT.md`. Output bytes are
  // parity-tested against the JS oracle below via
  // `crates/parity-runner --stage transform-css` and
  // `packages/css/scripts/verify-napi-transform-css.mjs`.
  //
  // The JS pipeline below stays as the parity oracle and the emergency
  // fallback for the next 12+ months per EXECUTION_PLAN.md Phase 10d —
  // do NOT delete or restructure it.
  //
  // Error wrapping: the Rust shim throws on parse / plugin failure with
  // the underlying message string. The JS wrapper's try/catch on
  // line 84-99 below re-wraps it in the same `createError('css',
  // 'Unhandled exception')` envelope used by the JS pipeline, so
  // consumers see the identical error shape on both engines.
  if (process.env.COMPILED_CSS_ENGINE === 'rust') {
    try {
      // eslint-disable-next-line @typescript-eslint/no-var-requires
      const { transformCss: rustTransformCss } = require('@sjcompiled/css-native');
      return rustTransformCss(css, {
        optimizeCss: opts.optimizeCss,
        classNameCompressionMap: opts.classNameCompressionMap,
        increaseSpecificity: opts.increaseSpecificity,
        sortAtRules: opts.sortAtRules,
        sortShorthand: opts.sortShorthand,
        classHashPrefix: opts.classHashPrefix,
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
    ${css}
  }

  Exception: ${message}`
      );
    }
  }

  const sheets: string[] = [];
  const classNames: string[] = [];

  try {
    const result = postcss([
      discardDuplicates(),
      discardEmptyRules(),
      parentOrphanedPseudos(),
      nested({
        bubble: [
          'container',
          '-moz-document',
          'layer',
          'else',
          'when',
          // postcss-nested bubbles `starting-style` by default in versions from 6.0.2 onwards:
          // https://github.com/postcss/postcss-nested?tab=readme-ov-file#bubble
          // When we upgrade to a version that includes this change, we can remove this from the list.
          'starting-style',
        ],
        unwrap: ['color-profile', 'counter-style', 'font-palette-values', 'page', 'property'],
      }),
      ...normalizeCSS(opts),
      expandShorthands(),
      atomicifyRules({
        classNameCompressionMap: opts.classNameCompressionMap,
        callback: (className: string) => classNames.push(className),
        classHashPrefix: opts.classHashPrefix,
      }),
      ...(opts.increaseSpecificity ? [increaseSpecificity()] : []),
      sortAtomicStyleSheet({
        sortAtRulesEnabled: opts.sortAtRules,
        sortShorthandEnabled: opts.sortShorthand,
      }),
      ...(process.env.AUTOPREFIXER === 'off' ? [] : [autoprefixer()]),
      whitespace(),
      extractStyleSheets({ callback: (sheet: string) => sheets.push(sheet) }),
    ]).process(css, {
      from: undefined,
    });

    // We need to access something to make the transformation happen.
    result.css;

    return {
      sheets,
      classNames: unique(classNames),
    };
  } catch (e: unknown) {
    const message = e instanceof Error ? e.message : e;
    throw createError(
      'css',
      'Unhandled exception'
    )(
      `An unhandled exception was raised when parsing your CSS, this is probably a bug!
  Raise an issue here: https://github.com/atlassian-labs/compiled/issues/new?assignees=&labels=&template=bug_report.md&title=CSS%20Parsing%20Exception:%20

  Input CSS: {
    ${css}
  }

  Exception: ${message}`
    );
  }
};
