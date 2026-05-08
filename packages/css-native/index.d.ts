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
  /**
   * Optional postcard-encoded prefix tables produced by
   * `precomputePrefixesDefault()`. When supplied, the autoprefixer
   * step skips its per-call filesystem walk + browserslist resolution
   * + full PREFIXES iteration. Byte-equal to omitting it.
   *
   * Designed for the WASI / SWC plugin call site where caching across
   * calls is impossible (per-call linear-memory teardown). Node
   * consumers can also benefit — call `precomputePrefixesDefault()`
   * once at process startup, pass the bytes on every `transformCss`.
   */
  precomputedPrefixes?: Buffer;
  /**
   * Filesystem-path delivery for the precomputed snapshot. The file
   * is read on each `transformCss` call. Designed for the WASI host
   * pattern: write the snapshot to a known path once per build, and
   * every plugin instance reads from there on each call (the OS page
   * cache amortises the read).
   *
   * Inline `precomputedPrefixes` takes precedence when both are set.
   * Read failure is surfaced as a `transformCss` error — NOT a silent
   * fallback to the slow path — so production config errors don't
   * hide behind a 100x perf regression.
   */
  precomputedPrefixesPath?: string;

  /**
   * Optional postcard-encoded browserslist snapshot produced by
   * `precomputeBrowserslistDefault()`. When supplied, the 5
   * browserslist-aware cssnano plugins (`postcss-reduce-initial`,
   * `-colormin`, `-convert-values`, `-minify-params`,
   * `-normalize-unicode`) skip their in-process
   * `browserslist_shim::resolve("")` paths and consume the host-
   * resolved snapshot directly.
   *
   * **Required for correct WASI behaviour with non-default
   * browserslist configs.** Inside WASI, env vars
   * (`BROWSERSLIST` / `BROWSERSLIST_CONFIG`) and FS walks for
   * `.browserslistrc` are unreachable — the leaf plugins fall back
   * to the wide `browserslist@4.24.2` defaults (which include IE 11)
   * and produce different output than the host's modern browser
   * list. See `DEFINITIVE_BROWSERSLIST_PLAN.md`.
   *
   * NAPI consumers don't strictly need this: the in-process shim
   * resolves correctly given the host's cwd / env. Pass it anyway
   * for parity with the WASI path (and for a small perf win — the
   * in-plugin resolution is ~200 µs/call).
   */
  precomputedBrowserslist?: Buffer;

  /**
   * Filesystem-path delivery for the `PrecomputedBrowserslist`
   * snapshot. Mirrors `precomputedPrefixesPath`.
   *
   * Inline `precomputedBrowserslist` takes precedence when both are
   * set. Read failure is a hard error.
   */
  precomputedBrowserslistPath?: string;
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

/**
 * Build the postcard prefix-tables blob once. Pass the returned
 * `Buffer` back via `transformCss(css, { precomputedPrefixes })` on
 * every call to skip the per-call autoprefixer setup cost.
 *
 * `from` mirrors `result.opts.from` — anchor for the `.browserslistrc`
 * walk. When omitted, resolution starts at `process.cwd()`.
 */
export function precomputePrefixesDefault(from?: string | null): Buffer;

/**
 * Build the postcard browserslist-snapshot blob once on the host.
 * Pass the returned `Buffer` back via
 * `transformCss(css, { precomputedBrowserslist })` (or write it to
 * a file and use `precomputedBrowserslistPath` for the WASI plugin).
 *
 * `from` is the filesystem anchor for the `.browserslistrc` upward
 * walk — pass the project root or a path under it. When omitted,
 * resolution starts at `process.cwd()`. The AFM canonical bootstrap
 * uses `require.resolve('postcss-reduce-initial/package.json')` so
 * the host-side walk-up is provably byte-equivalent to the leaf
 * plugin's own walk-up.
 *
 * The 5 cssnano plugins this snapshot drives:
 *   - postcss-reduce-initial (toInitial branch gating)
 *   - postcss-colormin (transparent_default + caniuse-rrggbbaa)
 *   - postcss-convert-values (keepZeroPercent for IE 11)
 *   - postcss-minify-params (legacy IE 10/11 bug detection)
 *   - postcss-normalize-unicode (lowercase u+ prefix on IE/Edge ≤15)
 */
export function precomputeBrowserslistDefault(from?: string | null): Buffer;
