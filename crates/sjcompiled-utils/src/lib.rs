//! crates/sjcompiled-utils
//! Byte-for-byte Rust port of `@sjcompiled/utils` (`packages/utils/src/`).
//!
//! Folder/file mapping (1:1 with `packages/utils/src/`):
//!   - `hash.ts`                    -> `src/hash.rs`           **CRITICAL**
//!   - `array.ts`                   -> `src/array.rs`
//!   - `kebab-case.ts`              -> `src/kebab_case.rs`
//!   - `to-boolean.ts`              -> `src/to_boolean.rs`
//!   - `error.ts`                   -> `src/error.rs`
//!   - `increase-specificity.ts`    -> `src/increase_specificity.rs`
//!   - `constants.ts`               -> `src/constants.rs`
//!   - `shorthand.ts`               -> `src/shorthand.rs`
//!   - `index.ts`                   -> `src/lib.rs` (this file)
//!
//! Files we deliberately don't port: `jsx.ts` (JSX regex used by the babel
//! plugin only), `default-parser-babel-plugins.ts` (babel-plugin internal),
//! `preserve-leading-comments.ts` (babel-plugin internal). None reach the
//! CSS hashing path. Add ports later if babel-plugin lands in Rust.

pub mod hash;
pub mod array;
pub mod kebab_case;
pub mod to_boolean;
pub mod error;
pub mod increase_specificity;
pub mod constants;
pub mod shorthand;

pub use hash::hash;
pub use array::{flatten, unique};
pub use kebab_case::kebab_case;
pub use to_boolean::to_boolean;
pub use error::create_error;
pub use increase_specificity::INCREASE_SPECIFICITY_SELECTOR;
pub use constants::{COMPILED_IMPORT, DEFAULT_IMPORT_SOURCES};
pub use shorthand::{shorthand_for, shorthand_buckets, ShorthandProperty};
