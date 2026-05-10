//! `compat::wasi_path` — host-absolute → WASI preopen path translation.
//!
//! ## Why this exists
//!
//! Upstream `packages/babel-plugin` runs inside a Node.js process with
//! full filesystem access. `resolve.sync(absolutePath, ...)` and
//! `fs.readFileSync(absolutePath, ...)` work against any host-absolute
//! path the host hands the plugin via Babel's `state.filename`.
//!
//! The Rust SWC plugin runs inside SWC's WASI runtime. Per
//! `crates/babel-plugin/PHASE0_FINDINGS.md`, the WASI sandbox grants
//! exactly ONE preopened directory: the host process's `current_dir`,
//! virtually mounted at **`/cwd`**. Any `std::fs::*` call against a
//! path outside `/cwd/...` returns `ENOTCAPABLE`. That's a
//! host-environment delta that has NO Babel/Node equivalent — there
//! is nothing in `packages/babel-plugin/src/utils/resolve-binding.ts`
//! to port from, because Node has no analogous sandbox.
//!
//! ## What this module does
//!
//! Translate a host-absolute path to its `/cwd/<rel-to-host-cwd>`
//! WASI form, given the host's project root (threaded in by the
//! host wrapper as `PluginOptions::root`, mirroring engines.ts and
//! the production Parcel transformer).
//!
//! ```text
//! host_root: /Users/me/proj
//! filename:  /Users/me/proj/fixtures/x/input.tsx
//! → wasi:    /cwd/fixtures/x/input.tsx
//! ```
//!
//! The translated path is what every downstream FS consumer
//! (`std::fs::read_to_string`, `oxc_resolver::Resolver::resolve`)
//! must see when running in the WASI sandbox. Native callers
//! (cargo unit tests, the in-process `run_dispatcher` integration
//! entrypoint) never go through translation — they get the host
//! path verbatim.
//!
//! ## Build-time mode dispatch (native vs WASI)
//!
//! The public surface ([`host_to_wasi`], [`host_to_wasi_path`],
//! [`relative_request_is_symlink`]) dispatches on
//! `cfg(target_arch = "wasm32")`:
//!
//! - **`wasm32-wasip1`** (the SWC WASI plugin binary) — runs the full
//!   translation/guard logic via `*_impl` helpers below.
//! - **Any other target** (the `crates/swc-native` consumer, cargo
//!   test, future native bindings) — short-circuits to the identity
//!   transform / `false` guard.
//!
//! This is a build-time switch, not a runtime flag, because the WASI
//! and native binaries are produced by genuinely separate `cargo
//! build` invocations targeting different triples. There is no
//! single binary that needs to support both modes, so a `cfg`
//! discriminator gives us zero runtime cost, dead-code-elimination of
//! the unused arm, and a compile-time guarantee that a WASI-only path
//! literally cannot be reached from native (and vice versa).
//!
//! Tests run against the host arch, so the `*_impl` helpers are
//! exposed under `#[cfg(any(target_arch = "wasm32", test))]` and the
//! tests below import them directly to keep the WASI translation
//! logic exercised by `cargo test`.
//!
//! ## Why not let `oxc_resolver` figure it out
//!
//! `oxc_resolver` calls `std::fs::*` directly. It cannot
//! distinguish a host-absolute path that's outside the WASI
//! preopen from one that's inside; it just hits `ENOTCAPABLE` and
//! returns `NotFound`. The translation has to happen at the plugin
//! boundary BEFORE any FS-touching call.
//!
//! ## Drift discipline
//!
//! - This is **not** a behavioural change vs upstream Babel — the
//!   resolved path back from `oxc_resolver` is identical (modulo
//!   the `/cwd` prefix), and we strip the prefix transparently so
//!   downstream `imported_filename` strings match what Node would
//!   produce. **TODO:** confirm imported_filename round-tripping
//!   is idempotent for any consumer that hashes it; today no
//!   consumer does, so this is parity-safe.
//! - Native `cargo test` runs continue to use host-absolute paths
//!   (no `/cwd` mount exists on a real filesystem). The translation
//!   functions are no-ops when `host_root` is empty or doesn't
//!   prefix the input path.

use std::path::{Path, PathBuf};

/// The WASI preopen mount point per
/// `crates/babel-plugin/PHASE0_FINDINGS.md`. The host's
/// `current_dir` is virtually mounted at this path inside the
/// plugin's view of the filesystem.
pub const WASI_CWD_MOUNT: &str = "/cwd";

/// Translate a host-absolute path to its `/cwd/<rel>` WASI form.
///
/// - When `host_root` is empty, returns `path` unchanged. This is
///   the native-test branch where no translation is needed.
/// - When `path` already starts with `/cwd`, returns it unchanged
///   (idempotent — safe to call multiple times).
/// - When `host_root` prefixes `path`, replace the prefix with
///   `/cwd` and forward-slash-normalise.
/// - When `host_root` doesn't prefix `path`, returns `path`
///   unchanged. Downstream FS calls will fail with `NotFound` —
///   this is intentional: silently rewriting unknown paths would
///   mask real misconfiguration (e.g. a fixture pointing at a
///   directory outside the project root).
///
/// ## Build-time dispatch
///
/// On non-`wasm32` targets this is the identity transform (the
/// host-absolute path is what the native filesystem expects, so no
/// translation is needed or correct). On `wasm32` it delegates to
/// [`host_to_wasi_impl`] which performs the `/cwd`-prefix mapping.
pub fn host_to_wasi(path: &str, host_root: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Native callers (`crates/swc-native`, future bindings,
        // `cargo test`) read host-absolute paths directly via
        // `std::fs::*`. There is no `/cwd` preopen to translate
        // into, and silently inserting `/cwd/` would break
        // every downstream `fs::read_to_string` on a real OS.
        let _ = host_root;
        path.to_string()
    }
    #[cfg(target_arch = "wasm32")]
    {
        host_to_wasi_impl(path, host_root)
    }
}

/// Path-form variant of [`host_to_wasi`] for callers holding
/// `PathBuf` / `&Path` values (e.g. the post-resolution path
/// returned by `oxc_resolver`).
pub fn host_to_wasi_path(path: &Path, host_root: &str) -> PathBuf {
    PathBuf::from(host_to_wasi(&path.to_string_lossy(), host_root))
}

/// WASI-target-only translation core. Exposed under `cfg(test)` so
/// the host-arch unit tests below can exercise the translation logic
/// directly.
#[cfg(any(target_arch = "wasm32", test))]
fn host_to_wasi_impl(path: &str, host_root: &str) -> String {
    if path.starts_with(WASI_CWD_MOUNT) {
        return path.to_string();
    }
    if host_root.is_empty() {
        return path.to_string();
    }
    let normalised_path = path.replace('\\', "/");
    let normalised_root = host_root.trim_end_matches('/').replace('\\', "/");
    if let Some(rest) = normalised_path.strip_prefix(&normalised_root) {
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        return if rest.is_empty() {
            WASI_CWD_MOUNT.to_string()
        } else {
            format!("{}/{}", WASI_CWD_MOUNT, rest)
        };
    }
    path.to_string()
}

/// WASI-only pre-resolution guard: detect a relative `request`
/// (e.g. `./foo`, `../foo`) that resolves to a **symlink** in the
/// `from_file`'s parent directory. Returns `true` when the
/// candidate path is unsafe to hand to
/// `oxc_resolver::Resolver::resolve` because doing so would hang
/// the plugin indefinitely.
///
/// ## Why this is necessary
///
/// Upstream `packages/babel-plugin` runs in Node.js and resolves
/// imports through `fs.realpathSync`, which throws cleanly on a
/// symlink that points to a missing file. The thrown error is
/// caught and the import deopts to the runtime CSS-variable
/// fallback.
///
/// The Rust port runs in SWC's WASI runtime, which grants the
/// plugin exactly one preopen (`/cwd`, the host's
/// `process.cwd()`). Empirical testing under wasm32-wasip1 has
/// shown that **every path-stat-style WASI syscall**
/// (`path_filestat_get`, `path_readlink`, `path_open` with
/// follow-symlinks) hangs indefinitely when invoked on a path
/// whose entry is a symlink — regardless of whether the symlink
/// target is inside or outside the preopen, regardless of
/// whether the target exists. (See `HANG_BUG_REPORT.md`'s
/// debugging trail; reproducer at
/// `/tmp/_rovodev_symlink_repro/jira/input4.tsx`.)
///
/// `oxc_resolver` 11.x calls `metadata` on every candidate
/// during extension probing inside `Resolver::resolve` even with
/// `symlinks: false` set on `ResolveOptions`. So once a relative
/// import lands on a candidate path that's a symlink in the
/// preopen, `oxc_resolver` enters the hanging syscall and the
/// whole transform stalls.
///
/// We CANNOT pre-validate via `metadata` / `read_link` /
/// `try_exists` from inside the plugin — those all hang too. The
/// ONLY WASI syscall that returns dirent shape information
/// without following the symlink is `path_readdir`, exposed via
/// `std::fs::read_dir`. `read_dir`'s `DirEntry::file_type()`
/// reports `is_symlink()` correctly without ever opening the
/// entry.
///
/// ## Strategy
///
/// For relative requests:
///
/// 1. Open the request's resolved parent directory via
///    `read_dir`.
/// 2. Walk entries until we find one whose name matches the
///    request's last segment (with each common code extension
///    appended).
/// 3. If the matching entry is a symlink, return `true` —
///    we MUST short-circuit before `oxc_resolver` touches it.
///
/// ## Drift discipline
///
/// This is an over-approximation: it treats EVERY symlink in the
/// import path as a hang risk and deopts. Upstream Babel
/// distinguishes "symlink with reachable target" (folds) from
/// "symlink with broken / missing target" (throws → deopts).
/// Under the WASI runtime we cannot make that distinction
/// without invoking the very syscall that hangs, so we collapse
/// both to deopt.
///
/// **Drift impact:** anywhere a fold relied on a symlink-based
/// import path, the WASM-built plugin will deopt to the runtime
/// fallback. The downstream visible diff is `var(--…)` /
/// `ix(...)` runtime style instead of an inlined literal. This
/// is the SAME shape Babel produces when the symlink chain
/// fails for any reason (escape, dangling, permission), so the
/// fallback is upstream-faithful — just hit on a wider set of
/// inputs in WASI mode.
///
/// **Native callers** (`opts.root` unset → `host_root = ""`)
/// skip this guard entirely — they're not in the WASI sandbox
/// and never hit the hanging syscall. `cargo test` runs against
/// real symlinks continue to work via the normal `oxc_resolver`
/// path.
///
/// **Bare-package imports** (`@compiled/react`, `lodash`, etc.)
/// are not handled by this guard. `oxc_resolver`'s node_modules
/// walk handles them via package.json resolution, which doesn't
/// route through symlink-stat on the request path itself. Bare
/// imports continue to flow through `oxc_resolver` unchanged.
pub fn relative_request_is_symlink(
    from_file: &Path,
    request: &str,
    host_root: &str,
) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Native callers run on a real filesystem with working
        // `realpath` / `path_filestat` semantics — `oxc_resolver`'s
        // canonical path follows the symlink chain in bounded time
        // (no WASI hang). The over-approximation that deopts every
        // symlinked import is therefore actively wrong on native:
        // it would force a `var(--…)` runtime fallback for cases
        // Babel folds. Returning `false` here lets `oxc_resolver`
        // resolve the symlink the way Node's `realpathSync` would.
        // See `crates/swc-native/tests/no_hang_on_unreadable_imports.rs`
        // for the regression guard that proves bounded-time
        // resolution on native against the WASI hang reproducer.
        let _ = (from_file, request, host_root);
        false
    }
    #[cfg(target_arch = "wasm32")]
    {
        relative_request_is_symlink_impl(from_file, request, host_root)
    }
}

/// WASI-target-only guard core. Exposed under `cfg(test)` so the
/// host-arch unit tests below can exercise the symlink-detection
/// logic directly against a real `tempdir`-staged symlink.
#[cfg(any(target_arch = "wasm32", test))]
fn relative_request_is_symlink_impl(
    from_file: &Path,
    request: &str,
    host_root: &str,
) -> bool {
    if host_root.is_empty() {
        return false;
    }
    let is_relative = request.starts_with("./")
        || request.starts_with("../")
        || request == "."
        || request == "..";
    if !is_relative {
        return false;
    }
    let parent = match from_file.parent() {
        Some(p) => p,
        None => return false,
    };
    // Resolve `parent + request` lexically — DO NOT call any FS
    // syscall yet. `target_path` is the candidate without any
    // extension suffix.
    let target_path = normalise_path(&parent.join(request));
    let target_dir = match target_path.parent() {
        Some(p) => p.to_path_buf(),
        None => return false,
    };
    let target_basename = match target_path.file_name() {
        Some(b) => b.to_string_lossy().to_string(),
        None => return false,
    };
    // Read the directory containing the candidate. `read_dir` uses
    // `path_readdir` which returns dirent shape info without
    // following any symlinks (the only WASI syscall that doesn't
    // hang on symlinked entries — see module-level docs).
    let entries = match std::fs::read_dir(&target_dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    // Match basename + any of the common code extensions. Mirrors
    // the extension list in `crate::constants::DEFAULT_CODE_EXTENSIONS`
    // — kept inline so this file has no dependency on `constants`.
    let extensions: &[&str] = &["", ".tsx", ".ts", ".jsx", ".js", ".mjs", ".cjs"];
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        for ext in extensions {
            let candidate_name = if ext.is_empty() {
                target_basename.clone()
            } else {
                format!("{}{}", target_basename, ext)
            };
            if name_str == candidate_name.as_str() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_symlink() {
                        return true;
                    }
                    return false;
                }
            }
        }
    }
    false
}

/// Lexical `..` / `.` collapse — does NOT touch the filesystem.
/// Used by [`relative_request_is_symlink_impl`] to compute the
/// parent dir + basename of a relative request without invoking
/// `canonicalize` (which hangs in WASI). Gated to the same target
/// set as the guard's `_impl`: WASI builds always need it; native
/// builds only need it when running the `cargo test` suite that
/// exercises the WASI logic against a real symlink fixture.
#[cfg(any(target_arch = "wasm32", test))]
fn normalise_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out: Vec<Component> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                if !matches!(out.last(), Some(Component::RootDir) | None)
                    && !matches!(out.last(), Some(Component::ParentDir))
                {
                    out.pop();
                }
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out.iter().collect()
}


#[cfg(test)]
mod tests {
    // Tests target the `_impl` helpers directly so the WASI
    // translation/guard logic stays exercised by `cargo test` even
    // when the host arch is non-wasm32 (where the public surface
    // short-circuits to identity / `false`). The native-mode
    // short-circuit itself is covered separately by
    // `native_mode_*` tests below and the `crates/swc-native`
    // integration test that proves bounded-time resolution.
    use super::host_to_wasi_impl as host_to_wasi;
    use super::relative_request_is_symlink_impl as relative_request_is_symlink;
    use super::*;

    #[test]
    fn translates_host_absolute_to_cwd_relative() {
        assert_eq!(
            host_to_wasi(
                "/Users/me/proj/fixtures/x/input.tsx",
                "/Users/me/proj",
            ),
            "/cwd/fixtures/x/input.tsx"
        );
    }

    #[test]
    fn translates_with_trailing_slash_in_root() {
        assert_eq!(
            host_to_wasi(
                "/Users/me/proj/fixtures/x/input.tsx",
                "/Users/me/proj/",
            ),
            "/cwd/fixtures/x/input.tsx"
        );
    }

    #[test]
    fn already_cwd_is_idempotent() {
        assert_eq!(
            host_to_wasi("/cwd/fixtures/x/input.tsx", "/Users/me/proj"),
            "/cwd/fixtures/x/input.tsx"
        );
    }

    #[test]
    fn empty_root_passes_through() {
        assert_eq!(
            host_to_wasi("/Users/me/proj/x.tsx", ""),
            "/Users/me/proj/x.tsx"
        );
    }

    #[test]
    fn non_prefix_passes_through() {
        // Outside the project root → unchanged. Downstream FS will
        // ENOTCAPABLE; that's the right diagnostic for a
        // misconfigured root or a fixture symlinked outside.
        assert_eq!(
            host_to_wasi("/elsewhere/foo.tsx", "/Users/me/proj"),
            "/elsewhere/foo.tsx"
        );
    }

    #[test]
    fn root_exactly_equal_returns_mount() {
        assert_eq!(host_to_wasi("/Users/me/proj", "/Users/me/proj"), "/cwd");
    }

    #[test]
    fn native_mode_host_to_wasi_is_identity() {
        // Build-time guarantee: the public `host_to_wasi` MUST be
        // an identity transform on the host arch. Catches a
        // regression where the cfg-gate breaks and the WASI body
        // is reached on native (which would silently break every
        // cross-file FS read in `swc-native`).
        assert_eq!(
            super::host_to_wasi(
                "/Users/me/proj/fixtures/x/input.tsx",
                "/Users/me/proj",
            ),
            "/Users/me/proj/fixtures/x/input.tsx"
        );
        assert_eq!(
            super::host_to_wasi("C:\\Users\\me\\x.tsx", "C:\\Users\\me"),
            "C:\\Users\\me\\x.tsx"
        );
    }

    #[test]
    fn native_mode_symlink_guard_always_false() {
        // Build-time guarantee: the public guard MUST return
        // `false` on the host arch regardless of inputs, so
        // `oxc_resolver`'s native realpath path runs unhindered.
        let from = Path::new("/anywhere/input.tsx");
        assert!(!super::relative_request_is_symlink(
            from,
            "./alias",
            "/Users/me/proj"
        ));
        assert!(!super::relative_request_is_symlink(
            from,
            "../escaping",
            "/Users/me/proj"
        ));
    }

    #[test]
    fn symlink_guard_no_op_when_host_root_empty() {
        // Native callers (`opts.root = None`) get a no-op guard so
        // `cargo test` runs continue to work against real symlinks.
        let from = Path::new("/anywhere/input.tsx");
        assert!(!relative_request_is_symlink(from, "./other", ""));
        assert!(!relative_request_is_symlink(from, "../other", ""));
    }

    #[test]
    fn symlink_guard_skips_bare_imports() {
        // Bare-package requests are not in scope — `oxc_resolver`
        // walks node_modules for these. The guard returns `false`
        // so `oxc_resolver` runs normally.
        let from = Path::new("/cwd/input.tsx");
        assert!(!relative_request_is_symlink(
            from,
            "@compiled/react",
            "/cwd"
        ));
        assert!(!relative_request_is_symlink(from, "lodash/fp", "/cwd"));
    }

    #[test]
    fn symlink_guard_passes_through_missing_relative_import() {
        // A missing `./foo` (no candidate exists) is safe to hand
        // to `oxc_resolver` — it returns `NotFound` cleanly.
        let from = Path::new("/cwd/__nonexistent_dir__/input.tsx");
        assert!(!relative_request_is_symlink(
            from,
            "./never-exists",
            "/cwd"
        ));
    }

    #[test]
    fn symlink_guard_detects_real_symlink_in_parent_dir() {
        // Real-FS reproducer: stage a directory with one regular
        // file and one symlink. `relative_request_is_symlink` must
        // return `true` when the request resolves to the symlinked
        // entry, `false` for the regular file.
        let tmp = std::env::temp_dir().join("rb_symlink_guard_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("real-target.tsx"), "export const x = 1;\n")
            .unwrap();
        let link = tmp.join("alias.tsx");
        let target = std::path::PathBuf::from("./real-target.tsx");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        #[cfg(not(unix))]
        {
            let _ = link;
            let _ = target;
            return;
        }

        let from = tmp.join("input.tsx");
        let host_root = tmp.to_string_lossy().to_string();
        // Symlink: must trigger the guard (we collapse all symlink
        // entries to `deopt` because the WASI runtime hangs on
        // path-stat regardless of target reachability — see
        // function-level docs).
        assert!(
            relative_request_is_symlink(&from, "./alias", &host_root),
            "symlinked entry must trigger the guard"
        );
        // Regular file at the same dir: must NOT trigger.
        assert!(
            !relative_request_is_symlink(&from, "./real-target", &host_root),
            "regular file must NOT trigger the guard"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn windows_backslash_root_normalises() {
        // engines.ts already replaces backslashes; defensive
        // normalisation here covers any host wrapper that doesn't.
        assert_eq!(
            host_to_wasi(
                "C:/Users/me/proj/x.tsx",
                "C:\\Users\\me\\proj",
            ),
            "/cwd/x.tsx"
        );
    }
}
