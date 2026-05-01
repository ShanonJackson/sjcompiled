//! crates/browserslist-shim
//! Wraps `oxc_browserslist` to mirror `browserslist@4.24.4` defaults +
//! config resolution. See `crates/PARITY_VERSIONS.md` Anomaly #4 — defaults
//! are version-specific.
//!
//! Folder/file mapping (1:1 with `node_modules/browserslist/`):
//!   - `index.js`        -> `src/index.rs`
//!   - `node.js`         -> `src/node.rs`
//!   - `parse.js`        -> `src/parse.rs`
//!   - `error.js`        -> `src/error.rs`
//!
//! NOTE: Phase 2d scaffold — full config resolution (BROWSERSLIST,
//! BROWSERSLIST_CONFIG, BROWSERSLIST_ENV, BROWSERSLIST_DISABLE_CACHE,
//! BROWSERSLIST_STATS, .browserslistrc, package.json field) pending.

pub mod index;
pub mod node;
pub mod parse;
pub mod error;

pub use error::BrowserslistError;
pub use index::resolve;
