//! Port of `packages/utils/src/constants.ts`.
//!
//! The `@compiled/` prefix matches the post-rename JS source — the
//! upstream `@compiled/` identifiers were renamed when this fork was
//! published. See `plugins/STATUS.md` "End-of-session notes" for the
//! rename context. Drift between this constant and
//! `packages/utils/src/constants.ts` would silently break the
//! ImportDeclaration recogniser in `crates/babel-plugin` and the
//! `DEFAULT_IMPORT_SOURCES` test paths in
//! `packages/babel-plugin-strip-runtime`.

pub const COMPILED_IMPORT: &str = "@compiled/react";
pub const DEFAULT_IMPORT_SOURCES: &[&str] = &[COMPILED_IMPORT, "@atlaskit/css"];
