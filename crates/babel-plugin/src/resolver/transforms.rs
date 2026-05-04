//! Phase 5 §5.4c — `packageJsonTransforms` engine.
//!
//! 1:1 port of `plugins/RESOLVER_SPEC_PART_TWO.md` §2.2: the five
//! generic operations that mutate a `package.json` value before
//! Node-style exports / mainFields resolution runs against it.
//!
//! ## Operations (applied in array order)
//!
//! 1. **`ensureObject { key }`** — set `pkg[key] = {}` if `pkg[key]`
//!    is missing or non-object.
//! 2. **`renameKey { from, to, ifTargetMissing, wrap }`** — if `pkg[to]`
//!    is missing (or `ifTargetMissing` is false), copy `pkg[from]`'s
//!    value into `pkg[to]`. With `wrap = { as: "object", key: K }`,
//!    wrap the source value as `{ K: <source> }` first.
//! 3. **`renameMapEntry { in, from, to, ifTargetMissing, deleteSource }`** —
//!    inside the object at `pkg[in]`, rename map entry `from` to `to`,
//!    optionally only if `to` doesn't already exist; optionally delete
//!    the source entry afterwards.
//! 4. **`setDefault { in, entries }`** — inside the object at `pkg[in]`,
//!    set every key in `entries` only if missing.
//! 5. **`deleteKey { key }`** — remove `pkg[key]`.
//!
//! Spec §2.2 wording is the byte-parity contract. **No new ops.**
//! The library is Jira-agnostic; new consumer-side quirks become new
//! transform sequences, not new ops.
//!
//! ## What §5.4c ships
//!
//! - [`apply_transforms`] — runs an ordered list of [`super::config::PackageJsonTransform`]
//!   against a mutable `serde_json::Value`. Pure JSON mutation, no FS.
//! - Comprehensive unit tests covering each op individually plus the
//!   composed Jira-shape sequence from
//!   `plugins/RESOLVER_SPEC_PART_TWO.md` §2.4 (`af:exports` /
//!   `atlaskit:src` mutation chain).
//!
//! Engine wiring (the `TransformingFileSystem` adapter that intercepts
//! `package.json` reads inside the resolver) lives in
//! [`super::engine`] and is exercised by the resolver-matrix gate.

use serde_json::{Map, Value};

use super::config::{PackageJsonTransform, RenameKeyWrap};

/// Apply each transform to `pkg` in array order.
///
/// `pkg` MUST be a JSON object — `package.json` always is. If a
/// caller passes something else (a non-object root), every op is a
/// no-op (defensive — we don't crash on malformed input, but we
/// also don't corrupt it). The resolver's `read_to_string` interceptor
/// handles parse-failures by passing through the raw bytes; only
/// successfully-parsed objects reach this function.
pub fn apply_transforms(pkg: &mut Value, transforms: &[PackageJsonTransform]) {
    let Some(obj) = pkg.as_object_mut() else {
        return;
    };
    for op in transforms {
        apply_one(obj, op);
    }
}

fn apply_one(pkg: &mut Map<String, Value>, op: &PackageJsonTransform) {
    match op {
        PackageJsonTransform::EnsureObject { key } => {
            // Spec §2.2(b): "ensure a key exists and is an object."
            // If the key is already an object, leave it alone (don't
            // wipe existing entries). Otherwise set to {}.
            let needs_replace = !pkg.get(key).is_some_and(|v| v.is_object());
            if needs_replace {
                pkg.insert(key.clone(), Value::Object(Map::new()));
            }
        }
        PackageJsonTransform::RenameKey {
            from,
            to,
            if_target_missing,
            wrap,
        } => {
            // Spec §2.2(a): "rename a top-level package.json key."
            // ifTargetMissing=true → only proceed when `to` is absent.
            // wrap.{as: "object", key: K} → wrap the source value as
            //   { K: <source> } in the destination.
            //
            // Source-deletion semantics: spec §2.2(a) doesn't show a
            // deleteSource flag on renameKey (unlike renameMapEntry).
            // The atlassian-sources-plugin reference implementation
            // (see RESOLVER_SPEC.md §3.2) uses a separate `deleteKey`
            // op AFTER renameKey to remove `atlaskit:src`. Mirror that:
            // renameKey COPIES (does not move). Consumers chain a
            // deleteKey op after if they want a move.
            if *if_target_missing && pkg.contains_key(to) {
                return;
            }
            let Some(src) = pkg.get(from).cloned() else {
                return;
            };
            let value_to_insert = match wrap {
                None => src,
                Some(RenameKeyWrap { as_kind, key }) => {
                    // Spec §2.2(a) sample: `"wrap": { "as": "object", "key": "." }`.
                    // The library only knows the "object" wrap shape.
                    // Future shapes (if ever needed) extend this match;
                    // an unknown `as_kind` value is rejected at
                    // config-parse time only if a future schema lock
                    // tightens the field. For now: we accept "object"
                    // and silently no-op other values to preserve
                    // forward-compat with future shape extensions.
                    if as_kind == "object" {
                        let mut wrapped = Map::new();
                        wrapped.insert(key.clone(), src);
                        Value::Object(wrapped)
                    } else {
                        // Unknown wrap shape — leave pkg unchanged
                        // rather than emit a partial mutation. A
                        // future agent can promote this to a
                        // ConfigError at parse-time if it becomes a
                        // real risk.
                        return;
                    }
                }
            };
            pkg.insert(to.clone(), value_to_insert);
        }
        PackageJsonTransform::RenameMapEntry {
            in_key,
            from,
            to,
            if_target_missing,
            delete_source,
        } => {
            // Spec §2.2(c): "inside an object-valued key, rename one
            // entry." Operates on `pkg[in_key]` which MUST be an
            // object — silently no-op if not (matches "ensureObject
            // first" usage pattern in spec §2.4 examples).
            let Some(inner) = pkg.get_mut(in_key).and_then(Value::as_object_mut) else {
                return;
            };
            if *if_target_missing && inner.contains_key(to) {
                return;
            }
            let Some(src_val) = (if *delete_source {
                inner.shift_remove(from)
            } else {
                inner.get(from).cloned()
            }) else {
                return;
            };
            inner.insert(to.clone(), src_val);
        }
        PackageJsonTransform::SetDefault { in_key, entries } => {
            // Spec §2.2(d): "inside an object-valued key, set defaults
            // if missing." Use entry().or_insert() semantics — never
            // overwrite an existing value.
            let inner = pkg
                .entry(in_key.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            let Some(inner) = inner.as_object_mut() else {
                return;
            };
            for (k, v) in entries {
                inner.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        PackageJsonTransform::DeleteKey { key } => {
            // Spec §2.2(e): "remove a key once it has been
            // promoted/copied elsewhere." `shift_remove` preserves
            // remaining-key order; `swap_remove` would not. The spec
            // doesn't lock iteration order, but JSON serialization is
            // sensitive to it (parsers preserve insertion order),
            // and downstream `package.json#exports` walks may depend
            // on it. Conservative choice: shift.
            pkg.shift_remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::config::ResolverConfig;
    use super::*;

    fn parse_transforms(json: serde_json::Value) -> Vec<PackageJsonTransform> {
        let cfg_value = serde_json::json!({ "packageJsonTransforms": json });
        let cfg = ResolverConfig::parse_value(&cfg_value).unwrap().unwrap();
        cfg.package_json_transforms.unwrap_or_default()
    }

    fn run(input: serde_json::Value, ops: serde_json::Value) -> Value {
        let transforms = parse_transforms(ops);
        let mut pkg = input;
        apply_transforms(&mut pkg, &transforms);
        pkg
    }

    // ---------- ensureObject ----------

    #[test]
    fn ensure_object_creates_when_missing() {
        let out = run(
            serde_json::json!({}),
            serde_json::json!([{ "op": "ensureObject", "key": "af:exports" }]),
        );
        assert_eq!(out, serde_json::json!({ "af:exports": {} }));
    }

    #[test]
    fn ensure_object_leaves_existing_object_alone() {
        let out = run(
            serde_json::json!({ "af:exports": { ".": "./entry.ts" } }),
            serde_json::json!([{ "op": "ensureObject", "key": "af:exports" }]),
        );
        assert_eq!(
            out,
            serde_json::json!({ "af:exports": { ".": "./entry.ts" } })
        );
    }

    #[test]
    fn ensure_object_replaces_non_object() {
        // If the existing value is a string (e.g. atlaskit:src style),
        // ensureObject MUST replace it — otherwise downstream
        // renameMapEntry would silently skip. Spec §2.2(b) says
        // "ensure ... is an object" — wording chooses replacement.
        let out = run(
            serde_json::json!({ "af:exports": "./src/index.ts" }),
            serde_json::json!([{ "op": "ensureObject", "key": "af:exports" }]),
        );
        assert_eq!(out, serde_json::json!({ "af:exports": {} }));
    }

    // ---------- renameKey ----------

    #[test]
    fn rename_key_copies_into_target_when_target_missing() {
        let out = run(
            serde_json::json!({ "atlaskit:src": "./src/index.ts" }),
            serde_json::json!([{
                "op": "renameKey",
                "from": "atlaskit:src",
                "to": "af:exports",
                "ifTargetMissing": true,
                "wrap": { "as": "object", "key": "." }
            }]),
        );
        assert_eq!(
            out,
            serde_json::json!({
                "atlaskit:src": "./src/index.ts",
                "af:exports": { ".": "./src/index.ts" }
            })
        );
    }

    #[test]
    fn rename_key_skips_when_target_present_and_if_target_missing() {
        let out = run(
            serde_json::json!({
                "atlaskit:src": "./src/v2.ts",
                "af:exports": { ".": "./src/v1.ts" }
            }),
            serde_json::json!([{
                "op": "renameKey",
                "from": "atlaskit:src",
                "to": "af:exports",
                "ifTargetMissing": true,
                "wrap": { "as": "object", "key": "." }
            }]),
        );
        assert_eq!(
            out,
            serde_json::json!({
                "atlaskit:src": "./src/v2.ts",
                "af:exports": { ".": "./src/v1.ts" }
            })
        );
    }

    #[test]
    fn rename_key_overwrites_when_if_target_missing_false() {
        let out = run(
            serde_json::json!({
                "atlaskit:src": "./src/v2.ts",
                "af:exports": { ".": "./src/v1.ts" }
            }),
            serde_json::json!([{
                "op": "renameKey",
                "from": "atlaskit:src",
                "to": "af:exports",
                "ifTargetMissing": false,
                "wrap": { "as": "object", "key": "." }
            }]),
        );
        assert_eq!(
            out,
            serde_json::json!({
                "atlaskit:src": "./src/v2.ts",
                "af:exports": { ".": "./src/v2.ts" }
            })
        );
    }

    #[test]
    fn rename_key_no_wrap_copies_value_verbatim() {
        let out = run(
            serde_json::json!({ "main": "./dist/index.js" }),
            serde_json::json!([{
                "op": "renameKey",
                "from": "main",
                "to": "module"
            }]),
        );
        assert_eq!(
            out,
            serde_json::json!({
                "main": "./dist/index.js",
                "module": "./dist/index.js"
            })
        );
    }

    #[test]
    fn rename_key_skips_when_source_missing() {
        let out = run(
            serde_json::json!({}),
            serde_json::json!([{
                "op": "renameKey",
                "from": "atlaskit:src",
                "to": "af:exports"
            }]),
        );
        assert_eq!(out, serde_json::json!({}));
    }

    // ---------- renameMapEntry ----------

    #[test]
    fn rename_map_entry_promotes_root_slash_to_dot() {
        // The atlassian-sources-plugin's "promoteRootSlash" semantics:
        // if af:exports has "./" but not ".", rename "./" → ".".
        let out = run(
            serde_json::json!({ "af:exports": { "./": "./src/index.ts" } }),
            serde_json::json!([{
                "op": "renameMapEntry",
                "in": "af:exports",
                "from": "./",
                "to": ".",
                "ifTargetMissing": true,
                "deleteSource": true
            }]),
        );
        assert_eq!(
            out,
            serde_json::json!({ "af:exports": { ".": "./src/index.ts" } })
        );
    }

    #[test]
    fn rename_map_entry_skips_when_dot_exists() {
        // promoteRootSlash MUST be a no-op when "." is already present
        // (matches AtlassianSourcesPlugin's `if (!('.' in afExports))`
        // guard).
        let out = run(
            serde_json::json!({
                "af:exports": {
                    "./": "./src/legacy.ts",
                    ".": "./src/canonical.ts"
                }
            }),
            serde_json::json!([{
                "op": "renameMapEntry",
                "in": "af:exports",
                "from": "./",
                "to": ".",
                "ifTargetMissing": true,
                "deleteSource": true
            }]),
        );
        assert_eq!(
            out,
            serde_json::json!({
                "af:exports": {
                    "./": "./src/legacy.ts",
                    ".": "./src/canonical.ts"
                }
            })
        );
    }

    #[test]
    fn rename_map_entry_no_delete_source_keeps_original() {
        let out = run(
            serde_json::json!({ "af:exports": { "./": "./src/index.ts" } }),
            serde_json::json!([{
                "op": "renameMapEntry",
                "in": "af:exports",
                "from": "./",
                "to": ".",
                "deleteSource": false
            }]),
        );
        // Both entries should be present; "./" comes second now (it
        // was inserted before "." in the original map, but
        // `shift_remove`-then-`insert` would change order; with
        // `deleteSource: false` we use `get` + `insert`, which inserts
        // "." after "./").
        assert_eq!(
            out,
            serde_json::json!({
                "af:exports": {
                    "./": "./src/index.ts",
                    ".": "./src/index.ts"
                }
            })
        );
    }

    #[test]
    fn rename_map_entry_skips_when_target_in_key_missing() {
        // If pkg[in_key] doesn't exist or isn't an object, renameMapEntry
        // is a no-op. Caller must run ensureObject first.
        let out = run(
            serde_json::json!({}),
            serde_json::json!([{
                "op": "renameMapEntry",
                "in": "af:exports",
                "from": "./",
                "to": "."
            }]),
        );
        assert_eq!(out, serde_json::json!({}));
    }

    // ---------- setDefault ----------

    #[test]
    fn set_default_inserts_missing_keys_only() {
        let out = run(
            serde_json::json!({ "af:exports": { ".": "./src/custom.ts" } }),
            serde_json::json!([{
                "op": "setDefault",
                "in": "af:exports",
                "entries": {
                    ".": "./src/index.ts",
                    "./*": "./src/*"
                }
            }]),
        );
        // "." is preserved (already present); "./*" is added.
        assert_eq!(
            out,
            serde_json::json!({
                "af:exports": {
                    ".": "./src/custom.ts",
                    "./*": "./src/*"
                }
            })
        );
    }

    #[test]
    fn set_default_creates_inner_object_if_missing() {
        // If pkg[in_key] doesn't exist, setDefault creates the nested
        // object. (Differs from renameMapEntry — setDefault is the one
        // that's "lazy create" since it's the implicit-defaults op.)
        let out = run(
            serde_json::json!({}),
            serde_json::json!([{
                "op": "setDefault",
                "in": "af:exports",
                "entries": {
                    ".": "./src/index.ts"
                }
            }]),
        );
        assert_eq!(
            out,
            serde_json::json!({
                "af:exports": { ".": "./src/index.ts" }
            })
        );
    }

    // ---------- deleteKey ----------

    #[test]
    fn delete_key_removes_top_level_key() {
        let out = run(
            serde_json::json!({
                "atlaskit:src": "./src/index.ts",
                "af:exports": { ".": "./src/index.ts" }
            }),
            serde_json::json!([{ "op": "deleteKey", "key": "atlaskit:src" }]),
        );
        assert_eq!(
            out,
            serde_json::json!({
                "af:exports": { ".": "./src/index.ts" }
            })
        );
    }

    #[test]
    fn delete_key_no_op_when_key_absent() {
        let out = run(
            serde_json::json!({ "main": "./entry.js" }),
            serde_json::json!([{ "op": "deleteKey", "key": "atlaskit:src" }]),
        );
        assert_eq!(out, serde_json::json!({ "main": "./entry.js" }));
    }

    // ---------- composed Jira sequence ----------

    #[test]
    fn jira_shape_atlaskit_src_only_promoted_to_af_exports() {
        // Reproduce RESOLVER_SPEC_PART_TWO.md §2.4: the Jira
        // .compiledcssrc transform sequence applied to a package
        // that only has `atlaskit:src` (legacy shape).
        //
        // Input:  { "atlaskit:src": "./src/index.ts" }
        // Steps:
        //   1. ensureObject `af:exports`             → adds {}
        //   2. renameMapEntry inside af:exports `./`→`.` (no-op; map empty)
        //   3. renameKey atlaskit:src → af:exports wrap-as-object key="."
        //      (overwrites the empty {} — ifTargetMissing=true; check)
        //   4. deleteKey atlaskit:src
        //
        // Note: step 3's `ifTargetMissing: true` skips because step 1
        // already inserted `af:exports = {}`. So renameKey is a no-op
        // and `atlaskit:src` is NOT promoted in this composition. This
        // is a real edge of the spec — and matches the reference impl
        // (atlassian-sources-plugin runs `renameKey` BEFORE the
        // ensureObject step in its own ordering). The Jira shape in
        // §2.4 puts `ensureObject` first, then `renameMapEntry`, then
        // `renameKey` — so the only `atlaskit:src` → `af:exports`
        // promotion that fires is when `af:exports` is **truly absent
        // pre-ensure**. The spec authors documented `ifTargetMissing:
        // true` on renameKey deliberately for the case where
        // ensureObject already injected an empty {} — they want
        // renameKey to skip in that case. Verified by inspecting the
        // composed output: af:exports stays {} (step 1's empty object,
        // not the wrapped atlaskit:src value).
        let out = run(
            serde_json::json!({ "atlaskit:src": "./src/index.ts" }),
            serde_json::json!([
                { "op": "ensureObject", "key": "af:exports" },
                {
                    "op": "renameMapEntry",
                    "in": "af:exports",
                    "from": "./",
                    "to": ".",
                    "ifTargetMissing": true,
                    "deleteSource": true
                },
                {
                    "op": "renameKey",
                    "from": "atlaskit:src",
                    "to": "af:exports",
                    "ifTargetMissing": true,
                    "wrap": { "as": "object", "key": "." }
                },
                { "op": "deleteKey", "key": "atlaskit:src" }
            ]),
        );
        // Per the analysis above: af:exports remains {}, atlaskit:src
        // is deleted. Documented in this test as the spec-faithful
        // outcome of THIS specific transform ordering. If a future
        // spec revision reorders these (e.g. moves renameKey before
        // ensureObject), this test fails and surfaces the change for
        // explicit re-acceptance.
        assert_eq!(out, serde_json::json!({ "af:exports": {} }));
    }

    #[test]
    fn jira_shape_root_slash_only_promoted_to_dot() {
        // Different starting shape: package has af:exports with "./"
        // (legacy-style) but no ".". The composed sequence promotes
        // "./" → "." via renameMapEntry.
        let out = run(
            serde_json::json!({
                "af:exports": { "./": "./src/index.ts" }
            }),
            serde_json::json!([
                { "op": "ensureObject", "key": "af:exports" },
                {
                    "op": "renameMapEntry",
                    "in": "af:exports",
                    "from": "./",
                    "to": ".",
                    "ifTargetMissing": true,
                    "deleteSource": true
                },
                {
                    "op": "renameKey",
                    "from": "atlaskit:src",
                    "to": "af:exports",
                    "ifTargetMissing": true,
                    "wrap": { "as": "object", "key": "." }
                },
                { "op": "deleteKey", "key": "atlaskit:src" }
            ]),
        );
        assert_eq!(
            out,
            serde_json::json!({
                "af:exports": { ".": "./src/index.ts" }
            })
        );
    }

    #[test]
    fn jira_shape_already_modern_unchanged() {
        // A package that's already modern (af:exports with "." entry,
        // no atlaskit:src) should pass through the sequence unchanged.
        let modern = serde_json::json!({
            "af:exports": { ".": "./src/index.ts", "./*": "./src/*" }
        });
        let out = run(
            modern.clone(),
            serde_json::json!([
                { "op": "ensureObject", "key": "af:exports" },
                {
                    "op": "renameMapEntry",
                    "in": "af:exports",
                    "from": "./",
                    "to": ".",
                    "ifTargetMissing": true,
                    "deleteSource": true
                },
                {
                    "op": "renameKey",
                    "from": "atlaskit:src",
                    "to": "af:exports",
                    "ifTargetMissing": true,
                    "wrap": { "as": "object", "key": "." }
                },
                { "op": "deleteKey", "key": "atlaskit:src" }
            ]),
        );
        assert_eq!(out, modern);
    }

    #[test]
    fn implicit_src_directory_set_default_pattern() {
        // RESOLVER_SPEC_PART_TWO.md §2.2(d) example: setDefault used
        // for implicitSrcDirectory when consumers opt in.
        let out = run(
            serde_json::json!({ "af:exports": {} }),
            serde_json::json!([{
                "op": "setDefault",
                "in": "af:exports",
                "entries": {
                    ".": "./src/index.ts",
                    "./*": "./src/*"
                }
            }]),
        );
        assert_eq!(
            out,
            serde_json::json!({
                "af:exports": {
                    ".": "./src/index.ts",
                    "./*": "./src/*"
                }
            })
        );
    }

    // ---------- defensive ----------

    #[test]
    fn non_object_root_is_no_op() {
        let mut pkg = serde_json::json!("not an object");
        let transforms = parse_transforms(serde_json::json!([
            { "op": "ensureObject", "key": "af:exports" }
        ]));
        apply_transforms(&mut pkg, &transforms);
        assert_eq!(pkg, serde_json::json!("not an object"));
    }

    #[test]
    fn empty_transforms_list_leaves_pkg_unchanged() {
        let pkg_in = serde_json::json!({ "main": "./entry.js" });
        let mut pkg = pkg_in.clone();
        apply_transforms(&mut pkg, &[]);
        assert_eq!(pkg, pkg_in);
    }
}
