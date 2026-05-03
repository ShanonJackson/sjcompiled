//! 1:1 port of `packages/babel-plugin/src/utils/`.
//!
//! Phase 2 §2.1 — only `constants` is filled. The other utility
//! modules (`cache`, `evaluate-expression`, `resolve-binding`,
//! `traverse-expression`, `traversers`, `css-builders`, …) land in
//! Phase 3-5 per `plugins/STATUS.md`. Each one MUST be a 1:1 port of
//! its `.ts` sibling — see `plugins/PLAN.md` constraint 4.

pub mod constants;
