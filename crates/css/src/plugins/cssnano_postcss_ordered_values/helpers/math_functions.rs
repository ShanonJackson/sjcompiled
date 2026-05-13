//! Port of `src/lib/mathfunctions.js`.
//!
//! Upstream: `new Set(['calc', 'clamp', 'max', 'min'])`. Membership-only.

pub fn is_math_function(name: &str) -> bool {
    matches!(name, "calc" | "clamp" | "max" | "min")
}
