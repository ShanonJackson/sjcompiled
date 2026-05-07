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
pub fn host_to_wasi(path: &str, host_root: &str) -> String {
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

/// Path-form variant of [`host_to_wasi`] for callers holding
/// `PathBuf` / `&Path` values (e.g. the post-resolution path
/// returned by `oxc_resolver`).
pub fn host_to_wasi_path(path: &Path, host_root: &str) -> PathBuf {
    PathBuf::from(host_to_wasi(&path.to_string_lossy(), host_root))
}

#[cfg(test)]
mod tests {
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
