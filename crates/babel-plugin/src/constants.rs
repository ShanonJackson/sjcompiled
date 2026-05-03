//! 1:1 port of `packages/babel-plugin/src/constants.ts`.
//!
//! Data-only — no runtime logic. Every value here is a string constant
//! the visitor / handlers reference verbatim. Drift between the JS
//! literal and the Rust constant is a byte-parity bug; reviewers MUST
//! diff this file against the upstream `constants.ts` before approving
//! any change.

pub const DOM_PROPS_IDENTIFIER_NAME: &str = "__cmpldp";
pub const PROPS_IDENTIFIER_NAME: &str = "__cmplp";
pub const REF_IDENTIFIER_NAME: &str = "__cmplr";
pub const STYLE_IDENTIFIER_NAME: &str = "__cmpls";

pub const COMPILED_DIRECTIVE_DISABLE_LINE: &str = "@compiled-disable-line";
pub const COMPILED_DIRECTIVE_DISABLE_NEXT_LINE: &str = "@compiled-disable-next-line";
pub const COMPILED_DIRECTIVE_TRANSFORM_CSS_PROP: &str = "transform-css-prop";

pub const DEFAULT_CODE_EXTENSIONS: &[&str] = &[".js", ".jsx", ".ts", ".tsx"];
