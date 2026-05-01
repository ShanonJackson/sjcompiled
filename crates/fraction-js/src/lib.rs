//! crates/fraction-js
//! Byte-for-byte Rust port of `fraction.js@4.2.0`.
//! See `crates/PARITY_VERSIONS.md` — do not deviate from upstream behavior.
//!
//! Upstream source: `node_modules/fraction.js/fraction.js`.
//!
//! Folder/file mapping:
//!   - `fraction.js` -> `src/fraction.rs`
//!
//! All bugs of the upstream version are intentionally preserved.

pub mod fraction;

pub use fraction::{Fraction, FractionError};
