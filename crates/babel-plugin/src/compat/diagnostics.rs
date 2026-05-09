//! `compat::diagnostics` — user-facing warning emission for WASI-only
//! deopts that have no upstream Babel analogue.
//!
//! ## Why this exists
//!
//! Upstream `packages/babel-plugin` runs in Node with full filesystem
//! access. There is NO Babel code path that reports "I couldn't fold
//! this because of a sandbox limitation" — Node has no sandbox. The
//! Rust SWC plugin runs inside SWC's WASI runtime which imposes
//! constraints that the upstream Babel plugin never had to surface
//! (single `/cwd` preopen, no working stat-on-symlink syscall under
//! `wasm32-wasip1`).
//!
//! When our guards in `compat::wasi_path` deopt because of one of
//! these constraints, the user sees an unexplained `var(--…)` runtime
//! fallback in their output. Without a diagnostic, that's
//! indistinguishable from a real Compiled bug. This module routes a
//! one-line warning through SWC's [`swc_common::errors::HANDLER`] so
//! the host (Parcel, webpack-loader, Jest test runner, the parity
//! harness) sees it on stderr alongside the rest of the build's
//! diagnostics.
//!
//! ## Drift discipline
//!
//! These warnings fire ONLY on WASI-mode plugin runs (gated on the
//! same `host_root` non-empty check that the symlink guard uses).
//! Native callers (`cargo test`, `run_dispatcher` in-process
//! integration entry) skip the warning surface entirely so the
//! upstream-faithful bytes-only contract remains testable in
//! isolation.
//!
//! Per-transform de-dup uses a `thread_local!` set — SWC tears down
//! the WASI instance between transforms (CLAUDE.md "WASI/WASM
//! Compilation"), so the set naturally resets every transform without
//! us having to plumb it through `State`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;

use swc_core::common::errors::HANDLER;
use swc_core::common::DUMMY_SP;

thread_local! {
    /// Per-transform memo of `(category, key)` warnings already
    /// emitted. Reset at WASI-instance teardown (every transform),
    /// matching the cache lifetime contract.
    static EMITTED: RefCell<HashSet<(&'static str, String)>> =
        RefCell::new(HashSet::new());
}

fn emit_once(category: &'static str, key: String, message: String) {
    let already = EMITTED.with(|set| {
        let mut set = set.borrow_mut();
        if set.contains(&(category, key.clone())) {
            true
        } else {
            set.insert((category, key));
            false
        }
    });
    if already {
        return;
    }
    // `is_set()` gate because native unit tests instantiate State
    // without SWC's plugin runner setting up the HANDLER scoped-tls.
    // `swc_common::errors::HANDLER` is a `better_scoped_tls::ScopedKey`
    // whose `with` panics if no value is set; `is_set` is the safe
    // probe.
    // Two channels because SWC plugin hosts vary in how they
    // surface diagnostics:
    //
    // * `eprintln!` writes to WASI fd 2, which every host
    //   (`@swc/core`, Parcel, the parity-harness, raw `wasmtime`)
    //   forwards to its own stderr unconditionally. This is the
    //   guaranteed-visible channel.
    // * `HANDLER.struct_span_warn` adds a structured diagnostic
    //   that hosts wiring up an `Emitter` (e.g. `swc_cli`,
    //   `@swc/core` with `experimental.cacheRoot` + diagnostics
    //   options) can render with spans/colour. Hosts that don't
    //   wire an emitter silently drop it — but it's free if they
    //   do, so we always emit.
    //
    // The `category` prefix lets users grep their build logs and
    // distinguishes us from unrelated stderr noise. It's omitted
    // from the HANDLER message because struct_span_warn already
    // routes through SWC's labelled "warning:" prefix.
    eprintln!("warning: @compiled (WASI {category}): {message}");
    if HANDLER.is_set() {
        HANDLER.with(|h| {
            h.struct_span_warn(DUMMY_SP, &message).emit();
        });
    }
}

/// Warn that a relative import points to a symlink, and so the fold
/// has been deopted to runtime fallback.
///
/// Fires from the three guard sites in `resolve_binding.rs` and
/// `evaluate_expression.rs`. Suppressed in native runs (callers gate
/// on `host_root` non-empty before reaching this helper).
pub fn warn_symlink_deopt(filename: Option<&str>, request: &str, from_file: &Path) {
    let from_display = from_file.display().to_string();
    let key = format!("{}|{}", from_display, request);
    let message = format!(
        "@compiled: skipping cross-file fold of `{request}` from `{from}` — \
         the import target is a symlink and SWC's WASI runtime cannot stat \
         symlinked entries (every `metadata`/`readlink` syscall hangs). \
         Falling back to runtime style. To enable folding, point the import \
         at the symlink's real target, or pre-resolve symlinks in your \
         build pipeline before invoking the SWC plugin.{filename_suffix}",
        request = request,
        from = from_display,
        filename_suffix = filename
            .map(|f| format!(" (file: {f})"))
            .unwrap_or_default(),
    );
    emit_once("symlink_deopt", key, message);
}

/// Warn that a host-absolute path resolved by `oxc_resolver` is
/// outside the WASI `/cwd` preopen, so we can't read it and have
/// deopted the fold.
///
/// Reserved for future wiring at the post-resolve `read_to_string`
/// boundary. The function is exported now so the plumbing is in
/// place when we land that guard (see `HANG_BUG_REPORT.md`
/// "post-resolve" follow-ups).
#[allow(dead_code)]
pub fn warn_outside_cwd_preopen(filename: Option<&str>, resolved_path: &Path, host_root: &str) {
    let resolved_display = resolved_path.display().to_string();
    let key = resolved_display.clone();
    let message = format!(
        "@compiled: skipping cross-file fold of `{resolved}` — the resolved \
         path is outside the WASI `/cwd` preopen (host root: `{root}`). \
         SWC's WASI sandbox grants exactly one preopened directory, so any \
         file outside it is unreadable from inside the plugin. Falling back \
         to runtime style. To enable folding, ensure the imported file \
         lives under the host root passed to the plugin.{filename_suffix}",
        resolved = resolved_display,
        root = host_root,
        filename_suffix = filename
            .map(|f| format!(" (file: {f})"))
            .unwrap_or_default(),
    );
    emit_once("outside_cwd", key, message);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-mode call must not panic when HANDLER isn't installed.
    /// Mirrors the cargo-test environment.
    #[test]
    fn warn_symlink_deopt_is_safe_without_handler() {
        warn_symlink_deopt(
            Some("/tmp/x/input.tsx"),
            "./alias",
            Path::new("/tmp/x/input.tsx"),
        );
    }

    #[test]
    fn warn_outside_cwd_preopen_is_safe_without_handler() {
        warn_outside_cwd_preopen(
            Some("/tmp/x/input.tsx"),
            Path::new("/elsewhere/y.tsx"),
            "/tmp/x",
        );
    }
}
