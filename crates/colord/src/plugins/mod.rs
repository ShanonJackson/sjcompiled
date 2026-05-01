//! Port of `colord/plugins/*.js`.
//!
//! Folder/file mapping (1:1 with `node_modules/colord/plugins/`):
//!   - `a11y.js`       -> `a11y.rs`
//!   - `harmonies.js`  -> `harmonies.rs`
//!   - `hwb.js`        -> `hwb.rs`
//!   - `lab.js`        -> `lab.rs`
//!   - `minify.js`     -> `minify.rs`
//!   - `mix.js`        -> `mix.rs`
//!   - `names.js`      -> registered globally via `crate::names` (matches
//!                        upstream — `names` is opt-in but always loaded by
//!                        cssnano-postcss-colormin).

pub mod a11y;
pub mod harmonies;
pub mod hwb;
pub mod lab;
pub mod minify;
pub mod mix;
