//! Hacks tree.
//!
//! Each module is a 1:1 port of `crates/_vendor/autoprefixer-10.4.14/package/lib/hacks/<kebab>.js`.
//! Only the hacks that the upstream `@compiled/css` corpus exercises are
//! ported; the remaining ~50 upstream hack files are not needed for the
//! workloads this crate serves and are intentionally absent. Adding one
//! is a two-step change: implement the module here, then register it in
//! `prefixes.rs::register_hacks` (alphabetical by JS filename).

pub mod background_clip;
pub mod cross_fade;
pub mod intrinsic;
pub mod text_decoration;
pub mod text_decoration_skip_ink;
pub mod user_select;
