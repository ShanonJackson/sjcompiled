//! crates/css
//! Rust port of `@compiled/css`'s public surface — the orchestrators that
//! compose local plugins (`crates/compiled-css`) with third-party postcss
//! crates into a working pipeline.
//!
//! Folder/file mapping (1:1 with `packages/css/src/`):
//!   - `index.ts`                 -> `src/lib.rs` (this file — re-exports)
//!   - `transform.ts`             -> `src/transform.rs`
//!   - `sort.ts`                  -> `src/sort.rs`
//!   - `generate-compression-map.ts` -> `src/generate_compression_map.rs`
//!
//! The `plugins/` and `utils/` subtrees that live under `packages/css/src/`
//! are *not* mirrored here — they live in `crates/compiled-css/`. This crate
//! only orchestrates.
//!
//! ## Parity contract
//!
//! Every public function on this crate is the byte-for-byte oracle target.
//! Plugin authors implement plugins under `crates/compiled-css/src/plugins/`,
//! splice them into [`transform::transform_css`], and verify byte-equality
//! against the JS pipeline (`packages/css/src/transform.ts`) using the
//! parity-runner.

pub mod transform;
pub mod sort;
pub mod generate_compression_map;

pub use transform::{transform_css, TransformOpts, TransformResult};
pub use sort::{sort, SortOpts};
pub use generate_compression_map::{generate_compression_map, GenerateCompressionMapOpts};
