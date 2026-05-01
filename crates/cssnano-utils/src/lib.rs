//! crates/cssnano-utils
//! Byte-for-byte Rust port of `cssnano-utils@3.1.0`.
//! See `crates/PARITY_VERSIONS.md`.
//!
//! Folder/file mapping (1:1 with `node_modules/cssnano-utils/src/`):
//!   - `index.js`         -> `src/lib.rs` (this file)
//!   - `getArguments.js`  -> `src/get_arguments.rs`
//!   - `rawCache.js`      -> `src/raw_cache.rs`
//!   - `sameParent.js`    -> `src/same_parent.rs`

pub mod get_arguments;
pub mod raw_cache;
pub mod same_parent;

pub use get_arguments::get_arguments;
pub use raw_cache::raw_cache_plugin;
pub use same_parent::same_parent;
