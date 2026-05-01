//! crates/postcss-value-parser
//! Byte-for-byte Rust port of `postcss-value-parser@4.2.0` (singular).
//! See `crates/PARITY_VERSIONS.md`.
//!
//! Folder/file mapping (1:1 with `node_modules/postcss-value-parser/lib/`):
//!   - `index.js`     -> `src/lib.rs` (this file — public API)
//!   - `parse.js`     -> `src/parse.rs`
//!   - `walk.js`      -> `src/walk.rs`
//!   - `stringify.js` -> `src/stringify.rs`
//!   - `unit.js`      -> `src/unit.rs`

pub mod parse;
pub mod walk;
pub mod stringify;
pub mod unit;

pub use parse::{parse, Node, NodeKind};
pub use stringify::stringify;
pub use unit::{parse_unit, ParsedUnit};
pub use walk::walk;

#[cfg(test)]
mod roundtrip_tests {
    use super::*;

    fn assert_round_trip(input: &str) {
        let nodes = parse(input);
        let out = stringify(&nodes);
        assert_eq!(out, input, "round-trip mismatch\n  input:  {:?}\n  output: {:?}", input, out);
    }

    #[test] fn keyword() { assert_round_trip("red"); }
    #[test] fn px_value() { assert_round_trip("16px"); }
    #[test] fn space_separated() { assert_round_trip("1px 2px 3px 4px"); }
    #[test] fn comma_list() { assert_round_trip("a, b, c"); }
    #[test] fn function_call() { assert_round_trip("rgb(1, 2, 3)"); }
    #[test] fn calc() { assert_round_trip("calc(1px + 2px)"); }
    #[test] fn url_unquoted() { assert_round_trip("url(foo.png)"); }
    #[test] fn url_with_spaces() { assert_round_trip("url(  foo.png  )"); }
    #[test] fn quoted_string() { assert_round_trip("\"hello\""); }
    #[test] fn comment_value() { assert_round_trip("/* hi */"); }
    #[test] fn nested_function() { assert_round_trip("translate(calc(1px + 2px), 3px)"); }
}
