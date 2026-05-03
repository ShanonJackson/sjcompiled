//! 1:1 port of `packages/babel-plugin/src/utils/constants.ts`.
//!
//! Babel's `path.get(name)` is typed against the parent node's child
//! keys; `CONDITIONAL_PATHS` enumerates the two we walk for ternary /
//! logical / if-statement-style branching nodes. The Rust port doesn't
//! use string-keyed AST access — branches are matched on enum
//! variants — so this constant is here for documentation / 1:1
//! file-mapping completeness. If a future utility actually needs the
//! pair (e.g. a generic "walk both sides" helper), import this slice.

pub const CONDITIONAL_PATHS: &[&str] = &["consequent", "alternate"];
