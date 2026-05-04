//! 1:1 port of
//! `packages/babel-plugin/src/utils/compress-class-names-for-runtime.ts`.
//!
//! Compress class names based on `classNameCompressionMap`.
//! The compressed class name has format `_aaaa_a` (atomic-group prefix
//! plus a compressed name), expected by the runtime `ac` helper.
//!
//! Atomic class names produced by `crates/css` are guaranteed ASCII
//! (`_<8-char-hash>`). The JS source uses `String.prototype.slice`
//! which is char-indexed; for ASCII this is identical to a byte slice.
//! The Rust port uses `chars().skip().take()` so a non-ASCII input
//! degrades the same way JS would (truncated by char) instead of
//! panicking on a non-char-boundary byte slice.

use indexmap::IndexMap;

/// Compress `class_names` against the optional compression map.
///
/// `class_name_compression_map` is keyed by `class_name[1..]` (drops
/// the leading `_`). When a key is found, the result is
/// `format!("_{}_{}", class_name[1..5], compressed)`. Otherwise the
/// original class name passes through.
pub fn compress_class_names_for_runtime(
    class_names: Vec<String>,
    class_name_compression_map: Option<&IndexMap<String, String>>,
) -> Vec<String> {
    let Some(map) = class_name_compression_map else {
        return class_names;
    };
    class_names
        .into_iter()
        .map(|class_name| {
            // JS: classNameCompressionMap[className.slice(1)]
            let key: String = class_name.chars().skip(1).collect();
            match map.get(&key) {
                Some(compressed) => {
                    // JS: `_${className.slice(1, 5)}_${compressedClassName}`
                    let prefix: String = class_name.chars().skip(1).take(4).collect();
                    format!("_{}_{}", prefix, compressed)
                }
                None => class_name,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        let mut m = IndexMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), (*v).to_string());
        }
        m
    }

    #[test]
    fn passthrough_when_map_absent() {
        let names = vec!["_aaaabbbb".to_string(), "_ccccdddd".to_string()];
        let out = compress_class_names_for_runtime(names.clone(), None);
        assert_eq!(out, names);
    }

    #[test]
    fn compresses_known_entry_to_underscore_form() {
        let m = map(&[("aaaabbbb", "a")]);
        let out = compress_class_names_for_runtime(vec!["_aaaabbbb".into()], Some(&m));
        assert_eq!(out, vec!["_aaaa_a".to_string()]);
    }

    #[test]
    fn passthrough_for_missing_key() {
        let m = map(&[("aaaabbbb", "a")]);
        let out = compress_class_names_for_runtime(vec!["_zzzzwwww".into()], Some(&m));
        assert_eq!(out, vec!["_zzzzwwww".to_string()]);
    }

    #[test]
    fn mixed_compression() {
        let m = map(&[("aaaabbbb", "a"), ("ccccdddd", "b")]);
        let out = compress_class_names_for_runtime(
            vec!["_aaaabbbb".into(), "_zzzzwwww".into(), "_ccccdddd".into()],
            Some(&m),
        );
        assert_eq!(
            out,
            vec![
                "_aaaa_a".to_string(),
                "_zzzzwwww".to_string(),
                "_cccc_b".to_string(),
            ]
        );
    }
}
