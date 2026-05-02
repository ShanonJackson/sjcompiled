// Public surface of the Rust NAPI backend.
//
// Mirrors the SortOpts shape in `packages/css/src/sort.ts` exactly.
// The byte-parity contract is enforced by `crates/parity-runner` —
// see `Stage::Sort`.

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
