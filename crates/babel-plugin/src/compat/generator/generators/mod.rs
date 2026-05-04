//! 1:1 port of `@babel/generator@7.23.0/lib/generators/`.
//!
//! Each upstream `generators/<file>.js` maps to a Rust module here.
//! We omit the upstream files that are NOT reachable from our 5
//! call-site corpus:
//! - `flow.js` — Flow types; not used by Compiled consumers.
//! - `typescript.js` — TS-only AST nodes; the corpus uses the TS
//!   parser surface but the fixtures don't carry TS-only nodes.
//! - `classes.js`, `methods.js`, `modules.js`, `statements.js` —
//!   `generate(&Expr)` doesn't see Statement / Declaration nodes,
//!   so these are out of scope unless a future call site lands one.
//! - `base.js` — Program / BlockStatement / Directive; same scope rule.
//! - `index.js` — re-export aggregator; replaced by Rust's `pub use`.
//!
//! When a future fixture surfaces a node kind from one of the omitted
//! files, port that file 1:1 alongside the fixture.

pub mod expressions;
pub mod template_literals;
pub mod types;
