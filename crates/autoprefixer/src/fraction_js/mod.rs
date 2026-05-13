//! `fraction.js@4.2.0` — byte-for-byte Rust port.
//!
//! Moved into `crates/autoprefixer` on 2026-05-14: autoprefixer is the
//! only consumer (`resolution.rs` uses `Fraction` for the
//! `min/max-resolution` dpcm/dpi math). The standalone `crates/fraction-js`
//! has been deleted — see `crates/CONSOLIDATION_PLAN.md`. Parity oracle
//! lives at `crates/autoprefixer/tests/fraction_js/oracle.json`
//! (regen with `node crates/autoprefixer/tests/fraction_js/gen_oracle.cjs`).
//!
//! Upstream source: `crates/_vendor/fraction.js-4.2.0/package/fraction.js`.
//! All bugs of the upstream version are intentionally preserved.
//! See `crates/PARITY_VERSIONS.md` — do not deviate from upstream behavior.

pub mod fraction;

pub use fraction::{Fraction, FractionError, FractionInput};
