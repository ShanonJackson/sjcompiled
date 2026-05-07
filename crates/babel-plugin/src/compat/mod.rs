//! Babel-API compatibility shims that have no SWC equivalent.
//!
//! These modules exist because `swc_core`'s native facilities don't
//! produce byte-identical output to the equivalent Babel utilities,
//! and the babel-plugin port has to feed those bytes (via
//! `compiled-utils::hash`) into class-name generation. A divergence
//! of even one byte renames every class on the consumer side.
//!
//! Conventions for files added here (per `CLAUDE.md`):
//! - One module per upstream npm package, named after the npm package.
//! - The module's doc-block cites the exact upstream version pin in
//!   `crates/PARITY_VERSIONS.md` and the source-of-truth file in
//!   `node_modules/<pkg>/...`.
//! - The `pub fn` surface mirrors the upstream entry point shape so
//!   call sites in the porting code can be ported 1:1.

pub mod evaluation;
pub mod generator;
pub mod globals;
pub mod import_type_specifier;
pub mod is_prop_valid;
pub mod jsesc;
pub mod paren;
pub mod path;
pub mod scope;
pub mod template_literal_raw;
pub mod wasi_path;
