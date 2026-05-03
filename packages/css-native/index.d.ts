// Public surface of the Rust NAPI backend.
//
// `sort` mirrors the SortOpts shape in `packages/css/src/sort.ts` exactly.
// `autoprefixer` mirrors the inputs `packages/css/src/transform.ts:70`
// passes to `autoprefixer()` (no explicit opts — it relies on the cwd
// `.browserslistrc` walk, which we expose here as `from`). The
// byte-parity contract is enforced by `crates/parity-runner` —
// see `Stage::Sort` and `Stage::Autoprefixer`.

export interface SortOpts {
  sortAtRulesEnabled?: boolean;
  sortShorthandEnabled?: boolean;
}

/**
 * Sorts an atomic style sheet — Rust port of `sort()` from
 * `packages/css/src/sort.ts:13`. Output is byte-for-byte identical to
 * the JS implementation under the parity contract in
 * `crates/PARITY_VERSIONS.md`.
 *
 * Throws an Error on parse failure (matching upstream postcss).
 */
export function sort(stylesheet: string, opts?: SortOpts | null): string;

export interface AutoprefixerOpts {
  /**
   * Mirrors postcss's `result.opts.from`. Autoprefixer reads
   * `path.dirname(from)` and passes it to browserslist's `path` option
   * for the `.browserslistrc` directory walk-up. AFM passes the source
   * `.css` path here in production.
   *
   * When omitted, browserslist resolves from the process cwd, matching
   * `browserslist@4.24.2`'s `prepareOpts` defaulting (HANDOVER.md §6).
   */
  from?: string;
}

/**
 * Adds vendor prefixes — Rust port of `autoprefixer@10.4.14` from
 * `packages/css/src/transform.ts:70`. Output is byte-for-byte identical
 * to the JS implementation under the parity contract in
 * `crates/PARITY_VERSIONS.md`. Verified across 65 corpus entries by
 * `crates/parity-runner --stage autoprefixer`.
 *
 * Throws an Error on parse failure (matching upstream postcss).
 */
export function autoprefixer(stylesheet: string, opts?: AutoprefixerOpts | null): string;

export interface TransformOpts {
  optimizeCss?: boolean;
  /**
   * Insertion order is significant — atomicify-rules iterates this map
   * via for-in semantics, which V8 specs as own-enumeration order.
   * The Rust NAPI shim walks `Object.keys(opts.classNameCompressionMap)`
   * and re-builds an `IndexMap` in that order.
   */
  classNameCompressionMap?: Record<string, string>;
  increaseSpecificity?: boolean;
  sortAtRules?: boolean;
  sortShorthand?: boolean;
  classHashPrefix?: string;
}

export interface TransformResult {
  sheets: string[];
  classNames: string[];
}

/**
 * Full `transformCss(css, opts)` pipeline — Rust port of
 * `packages/css/src/transform.ts:32`. Composes 12 plugins in
 * postcss-lifecycle-correct order (Once → walk → OnceExit) per
 * `crates/PHASE_8B_LIFECYCLE_AUDIT.md`. Output is byte-for-byte
 * identical to the JS implementation under the parity contract in
 * `crates/PARITY_VERSIONS.md`.
 *
 * The `AUTOPREFIXER` env var is read at call time, matching JS:
 * `process.env.AUTOPREFIXER === 'off'` disables the autoprefixer step.
 *
 * Throws an Error on parse failure or plugin failure (matching upstream
 * postcss). The JS-side wrapper at `packages/css/src/transform.ts:84-99`
 * re-wraps any thrown error in a `createError('css', 'Unhandled
 * exception')` envelope; consumers calling the JS engine see that
 * envelope, consumers calling the Rust engine through this NAPI shim
 * see the underlying message directly. The JS wrapper handles both.
 */
export function transformCss(
  css: string,
  opts?: TransformOpts | null,
): TransformResult;
