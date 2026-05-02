//! crates/autoprefixer
//! Byte-for-byte Rust port of `autoprefixer@10.4.14`.
//! See `crates/PARITY_VERSIONS.md` — do not deviate from upstream behavior.
//!
//! # Source map (1:1)
//!
//! | Upstream JS                      | Rust                              |
//! |----------------------------------|-----------------------------------|
//! | `lib/autoprefixer.js`            | `src/autoprefixer.rs`             |
//! | `lib/processor.js`               | `src/processor.rs`                |
//! | `lib/prefixes.js`                | `src/prefixes.rs`                 |
//! | `lib/prefixer.js`                | `src/prefixer.rs`                 |
//! | `lib/browsers.js`                | `src/browsers.rs`                 |
//! | `lib/declaration.js`             | `src/declaration.rs`              |
//! | `lib/value.js`                   | `src/value.rs`                    |
//! | `lib/selector.js`                | `src/selector.rs`                 |
//! | `lib/at-rule.js`                 | `src/at_rule.rs`                  |
//! | `lib/resolution.js`              | `src/resolution.rs`               |
//! | `lib/supports.js`                | `src/supports.rs`                 |
//! | `lib/transition.js`              | `src/transition.rs`               |
//! | `lib/old-value.js`               | `src/old_value.rs`                |
//! | `lib/old-selector.js`            | `src/old_selector.rs`             |
//! | `lib/info.js`                    | `src/info.rs`                     |
//! | `lib/utils.js`                   | `src/utils.rs`                    |
//! | `lib/vendor.js`                  | `src/vendor.rs`                   |
//! | `lib/brackets.js`                | `src/brackets.rs`                 |
//! | `data/prefixes.js`               | `src/data/prefixes.rs`            |
//! | `lib/hacks/<kebab>.js`           | `src/hacks/<snake>.rs`            |
//!
//! # Split contract (parallel-agent boundary)
//!
//! See `src/hacks/HACKS_PORT.md`. The 58 files under `src/hacks/` are
//! the parallel agent's territory. Everything else is owned by the
//! foundation/core agent.

pub mod at_rule;
pub mod autoprefixer;
pub mod brackets;
pub mod browsers;
pub mod declaration;
pub mod info;
pub mod old_selector;
pub mod old_value;
pub mod prefixer;
pub mod prefixes;
pub mod processor;
pub mod resolution;
pub mod selector;
pub mod supports;
pub mod transition;
pub mod utils;
pub mod value;
pub mod vendor;

pub mod data {
    pub mod prefixes;
}

pub mod hacks;
