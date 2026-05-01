//! Stages: each value names ONE pipeline configuration to diff.
//!
//! As plugins land, add a variant here, wire it into `rust_run_stage`,
//! and add the matching JS counterpart in `scripts/js-pipeline.mjs`.

use postcss_core::{parse, stringify};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// `parse(css).toString()` — the postcss-core round-trip oracle.
    /// Used to confirm the parser+stringifier are byte-clean before any
    /// plugin layers it.
    PostcssCoreRoundtrip,

    /// `parse → discardEmptyRules → stringify`. The Phase 4a entry point.
    DiscardEmptyRules,
}

impl Stage {
    pub fn name(&self) -> &'static str {
        match self {
            Stage::PostcssCoreRoundtrip => "postcss-core-roundtrip",
            Stage::DiscardEmptyRules => "discard-empty-rules",
        }
    }
}

/// Run the Rust counterpart of `stage` against `css` and return the
/// stringified output. `Err` carries a description of why the Rust side
/// couldn't produce output (parse error, plugin error, etc.).
pub fn rust_run_stage(stage: Stage, css: &str) -> Result<String, String> {
    match stage {
        Stage::PostcssCoreRoundtrip => {
            let root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            Ok(stringify(&root))
        }
        Stage::DiscardEmptyRules => {
            let mut root = parse(css).map_err(|e| format!("rust parse error: {e}"))?;
            // The plugin body is in `crates/compiled-css/src/plugins/discard_empty_rules.rs`.
            // Until Phase 4a lands the plugin returns `unimplemented!()`;
            // catching panics turns that into a clean error here so the
            // harness can still report progress on inputs that the JS
            // side handles.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                compiled_css::plugins::discard_empty_rules::discard_empty_rules(&mut root);
            }));
            if result.is_err() {
                return Err("rust plugin panicked (likely unimplemented!)".to_string());
            }
            Ok(stringify(&root))
        }
    }
}
