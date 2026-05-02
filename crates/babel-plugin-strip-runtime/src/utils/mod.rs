//! 1:1 with `packages/babel-plugin-strip-runtime/src/utils/`.
//! File names switch from kebab-case to snake_case (Rust requirement);
//! folder structure stays identical.

pub mod is_automatic_runtime;
pub mod is_cc_component;
pub mod is_create_element;
pub mod remove_style_declarations;
pub mod to_uri_component;
