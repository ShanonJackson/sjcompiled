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
//!   - `jsx.ts`                     -> `src/jsx.rs` (added Phase 2 §2.3
//!                                     when the babel-plugin port started
//!                                     consuming `JSX_ANNOTATION_REGEX`)
//!
//! Files we deliberately don't port yet: `default-parser-babel-plugins.ts`
//! and `preserve-leading-comments.ts` are babel-plugin internals — they
//! land alongside the babel-plugin Rust port (`crates/babel-plugin/`)
//! when the dispatcher visitor needs them. None reach the CSS hashing
//! path.

pub mod hash;
pub mod array;
pub mod kebab_case;
pub mod to_boolean;
pub mod error;
pub mod increase_specificity;
pub mod constants;
pub mod shorthand;
pub mod jsx;

pub use hash::hash;
pub use array::{flatten, unique};
pub use kebab_case::kebab_case;
pub use to_boolean::to_boolean;
pub use error::create_error;
pub use increase_specificity::INCREASE_SPECIFICITY_SELECTOR;
pub use constants::{COMPILED_IMPORT, DEFAULT_IMPORT_SOURCES};
pub use shorthand::{shorthand_for, shorthand_buckets, ShorthandProperty};
pub use jsx::{jsx_annotation_regex, jsx_source_annotation_regex};
