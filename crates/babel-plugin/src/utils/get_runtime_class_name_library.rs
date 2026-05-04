//! 1:1 port of
//! `packages/babel-plugin/src/utils/get-runtime-class-name-library.ts`.
//!
//! Returns `"ac"` when `classNameCompressionMap` is set, `"ax"`
//! otherwise. `ac` does what `ax` does plus handling compressed class
//! names; `ax` is faster, so it's the default unless compression is
//! actually in use.

use crate::types::Metadata;

pub fn get_runtime_class_name_library(meta: &Metadata<'_>) -> &'static str {
    if meta.state.opts().class_name_compression_map.is_some() {
        "ac"
    } else {
        "ax"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::{MetadataContext, PluginOptions};
    use indexmap::IndexMap;

    fn meta_with(opts: PluginOptions, state: &mut State) -> Metadata<'_> {
        state.set_opts(opts);
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
        }
    }

    #[test]
    fn returns_ax_when_no_compression_map() {
        let mut state = State::default();
        let meta = meta_with(PluginOptions::default(), &mut state);
        assert_eq!(get_runtime_class_name_library(&meta), "ax");
    }

    #[test]
    fn returns_ac_when_compression_map_present() {
        let mut state = State::default();
        let mut map = IndexMap::new();
        map.insert("aaaabbbb".to_string(), "a".to_string());
        let opts = PluginOptions {
            class_name_compression_map: Some(map),
            ..Default::default()
        };
        let meta = meta_with(opts, &mut state);
        assert_eq!(get_runtime_class_name_library(&meta), "ac");
    }
}
