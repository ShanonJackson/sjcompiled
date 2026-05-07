//! Compatibility shims that bridge JavaScript runtime semantics into
//! Rust. Files in here exist solely so the Rust port observably matches
//! a JS engine's behaviour on the precise surface the upstream JS code
//! depends on. Each shim documents the JS source it mirrors and the
//! reason a vanilla Rust equivalent doesn't suffice.

pub mod v8_array_sort;
