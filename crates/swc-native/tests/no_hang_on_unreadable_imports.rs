//! Regression guard against the WASI symlink-canonicalisation hang
//! re-emerging on the native (`crates/swc-native`) transform.
//!
//! ## Background
//!
//! `HANG_BUG_REPORT.md` (deleted alongside the WASI fix in commit
//! `0793eb4`) documented an infinite loop in the WASI plugin when a
//! cross-file import resolved to a path the WASI sandbox could not
//! read — typically a relative `./foo` whose extension-probed
//! candidate was a symlink to a target outside the `/cwd` preopen.
//! Under `wasm32-wasip1`, every `path_filestat_get` /
//! `path_readlink` / `path_open(follow_symlinks=true)` syscall hangs
//! indefinitely on a symlinked entry, so `oxc_resolver`'s
//! canonicalisation walk never terminates and the whole transform
//! stalls. The WASI plugin's fix wires
//! `compat::wasi_path::relative_request_is_symlink` as a pre-resolve
//! deopt guard.
//!
//! On `crates/swc-native` the same babel-plugin code runs against a
//! real OS filesystem, where `realpath` / `path_filestat` work
//! normally — `oxc_resolver` follows the symlink in bounded time and
//! either returns the canonical target (when reachable) or
//! `NotFound` (when dangling). The build-time `cfg(target_arch)`
//! switch on `relative_request_is_symlink` and on the resolver's
//! `symlinks` option is what lets native take that bounded-time
//! path; this test asserts the switch is wired correctly and stays
//! that way.
//!
//! ## What this test does
//!
//! Stages the exact fixture layout the deleted `HANG_BUG_REPORT.md`
//! used (input/input3 baselines + input4/input5/input6/input7
//! escape variants), runs each through `swc_native::transform` on a
//! spawned worker thread with a 5-second `recv_timeout` fence, and
//! asserts each result lands within the timeout. The fence's
//! purpose is "hang detection without wedging CI" — a real hang
//! makes the channel's `recv_timeout` fire instead of letting the
//! test process deadlock until the OS kills it.
//!
//! ## Outcome semantics
//!
//! For the hang-reproducer inputs (4/5/6) we do NOT pin the
//! transform's output shape. Native's `symlinks: true` resolver
//! follows the link to its real target; for `escaping-link` /
//! `escaping-arrow` the target IS readable on a real FS, so native
//! folds the import (Babel-aligned) where WASI deopts. For
//! `dangling` the target doesn't exist, so the resolver returns
//! `NotFound` and the import deopts cleanly to the runtime
//! fallback. **Both folding and deopt are passing outcomes** — the
//! test only fails on hang.
//!
//! For the regression-baseline inputs (input/input3/input7) we do
//! assert "the transform completed" but again don't pin the bytes
//! — the parity-harness owns byte-equality coverage. This test's
//! sole job is to prove no hang.
//!
//! ## Windows
//!
//! Symlink creation on Windows requires Developer Mode or admin
//! rights. Cold-fail symlink creation makes the test self-skip
//! (with an `eprintln!`) rather than failing — matching the
//! pattern in `crates/babel-plugin/src/compat/wasi_path.rs:388`.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// 64 MiB — same as `examples/triage_dump.rs`. Some fixtures push
/// babel-plugin's recursion past 16 MiB on native; the small fixtures
/// here don't, but using a uniform stack avoids per-test guesswork.
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

/// 5 seconds. The hang reproducer used `perl -e 'alarm 15; ...'` for a
/// 15s WASI-side fence. Native should finish each fixture in well
/// under 100 ms — anything past 5 s is a hang, not a slow run.
const HANG_FENCE: Duration = Duration::from_secs(5);

/// Cross-platform symlink helper. Returns `Ok(())` on success,
/// `Err(io::Error)` when the OS rejects creation (notably Windows
/// without Developer Mode / admin). Callers self-skip the test on
/// `Err`.
fn symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symlink: unsupported platform",
        ))
    }
}

/// Stage the full fixture layout described in the deleted
/// `HANG_BUG_REPORT.md` (recoverable from commit `0793eb4`). Returns
/// the inner cwd (`<tmp>/jira`) — the path each input is anchored
/// to as `state.filename`.
///
/// Returns `Ok(None)` if symlink creation fails (Windows without
/// Developer Mode); the test self-skips in that case.
fn stage_repro(tmp_root: &Path) -> std::io::Result<Option<PathBuf>> {
    // <tmp>/jira/ — the inner project root. Inputs live here.
    let cwd = tmp_root.join("jira");
    std::fs::create_dir_all(&cwd)?;

    // <tmp>/escaped-constants.tsx — readable target for input4's
    // symlink. Lives OUTSIDE `cwd`, so on WASI this would have been
    // unreachable; on native it's just a normal file.
    std::fs::write(
        tmp_root.join("escaped-constants.tsx"),
        "export const layers = {\n  \
            card: function card() { return 100; },\n  \
            tooltip: function tooltip() { return 9999; },\n\
         };\n",
    )?;

    // <tmp>/escaped-constants-arrow.tsx — arrow-shape variant for
    // input5. Same structural shape, different callable form.
    std::fs::write(
        tmp_root.join("escaped-constants-arrow.tsx"),
        "export const layers = {\n  \
            card: () => 100,\n  \
            tooltip: () => 9999,\n\
         };\n",
    )?;

    // <tmp>/jira/local-constants.tsx — in-cwd baseline for input3.
    // Tests that the in-cwd fold path still works alongside the
    // out-of-cwd ones.
    std::fs::write(
        cwd.join("local-constants.tsx"),
        "export const layers = {\n  tooltip: () => 9999,\n};\n",
    )?;

    // Symlinks: jira/escaping-link.tsx → ../escaped-constants.tsx
    //          jira/escaping-arrow.tsx → ../escaped-constants-arrow.tsx
    //          jira/dangling.tsx → ../does-not-exist.tsx
    //
    // Symlink creation is the failure-prone step on Windows. If any
    // of these fails, return Ok(None) so the test self-skips with a
    // clear message.
    if let Err(e) = symlink_file(
        Path::new("../escaped-constants.tsx"),
        &cwd.join("escaping-link.tsx"),
    ) {
        eprintln!(
            "no_hang_on_unreadable_imports: skipping — symlink creation \
             not permitted ({e}). On Windows enable Developer Mode or \
             run as admin to exercise this regression guard."
        );
        return Ok(None);
    }
    symlink_file(
        Path::new("../escaped-constants-arrow.tsx"),
        &cwd.join("escaping-arrow.tsx"),
    )?;
    symlink_file(
        Path::new("../does-not-exist.tsx"),
        &cwd.join("dangling.tsx"),
    )?;

    // Input fixtures. Each imports `styled` from `@compiled/react`
    // (recognised by the plugin via source-string match — no actual
    // node_modules entry needed for the plugin to fire) and uses
    // `styled.div({ zIndex: layers.tooltip() })` so the cross-file
    // fold path is exercised on each `layers.tooltip()` call.
    let inputs: &[(&str, &str)] = &[
        // Regression baseline: bare-package import the resolver
        // can't find — clean deopt path, not exercising symlinks.
        (
            "input.tsx",
            "import { styled } from '@compiled/react';\n\
             import { layers } from '@atlaskit/theme/constants';\n\
             export const X = styled.div({ zIndex: layers.tooltip() });\n",
        ),
        // Regression baseline: in-cwd relative import — must complete.
        (
            "input3.tsx",
            "import { styled } from '@compiled/react';\n\
             import { layers } from './local-constants';\n\
             export const X = styled.div({ zIndex: layers.tooltip() });\n",
        ),
        // Hang reproducer: relative import → symlink → out-of-cwd
        // file. WASI hung here; native must complete in bounded time.
        (
            "input4.tsx",
            "import { styled } from '@compiled/react';\n\
             import { layers } from './escaping-link';\n\
             export const X = styled.div({ zIndex: layers.tooltip() });\n",
        ),
        // Hang reproducer: arrow-shape variant.
        (
            "input5.tsx",
            "import { styled } from '@compiled/react';\n\
             import { layers } from './escaping-arrow';\n\
             export const X = styled.div({ zIndex: layers.tooltip() });\n",
        ),
        // Hang reproducer: dangling symlink (target doesn't exist).
        // Native: resolver returns NotFound → clean deopt.
        (
            "input6.tsx",
            "import { styled } from '@compiled/react';\n\
             import { layers } from './dangling';\n\
             export const X = styled.div({ zIndex: layers.tooltip() });\n",
        ),
        // Regression baseline: explicit `..` path with no symlink in
        // the chain — the resolver lexically rejects it on WASI;
        // on native it succeeds.
        (
            "input7.tsx",
            "import { styled } from '@compiled/react';\n\
             import { layers } from '../escaped-constants';\n\
             export const X = styled.div({ zIndex: layers.tooltip() });\n",
        ),
    ];
    for (name, body) in inputs {
        std::fs::write(cwd.join(name), body)?;
    }

    Ok(Some(cwd))
}

/// Build the JSON options shape `swc_native::transform` expects.
/// Mirrors `examples/triage_dump.rs:156-192` — same options the
/// parity harness uses, so this test exercises the production
/// invocation shape.
fn build_opts_for(filename: &Path) -> Vec<u8> {
    let opts = serde_json::json!({
        "filename": filename.to_string_lossy(),
        "jsc": {
            "target": "es2022",
            "parser": { "syntax": "typescript", "tsx": true },
            "transform": {
                "verbatimModuleSyntax": true,
                "react": { "runtime": "classic" }
            },
            "preserveAllComments": false,
            "experimental": {
                "runPluginFirst": true,
                "plugins": [["babel_plugin.wasm", {
                    // No `root` — native's path-translation cfg-gate
                    // makes `host_to_wasi` an identity transform on
                    // non-wasm32 regardless, but we leave it `None`
                    // to mirror what `swc-native`'s production
                    // callers do today.
                    "optimizeCss": false,
                }]]
            }
        }
    });
    serde_json::to_vec(&opts).expect("serialize opts")
}

/// Run one transform on a worker thread, fenced by `HANG_FENCE`.
/// Returns:
///   * `Ok(Ok(code))` — transform completed within the fence.
///   * `Ok(Err(msg))` — transform completed but returned an error.
///                       Both folding and clean deopt are reported as
///                       `Ok(...)` here; only a panic / Rust-side
///                       error becomes `Err`.
///   * `Err(_)`        — HANG: the worker did not deliver a result
///                       within `HANG_FENCE`. This is the regression
///                       failure mode this test guards against.
fn transform_with_hang_fence(
    source: String,
    opts_bytes: Vec<u8>,
    fixture_name: &'static str,
) -> Result<Result<String, String>, mpsc::RecvTimeoutError> {
    let (tx, rx) = mpsc::channel();
    let _join = thread::Builder::new()
        .name(format!("hang-fence:{fixture_name}"))
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || {
            // `catch_unwind` so a Rust-side panic in the transform
            // becomes a structured error rather than killing the
            // test process. The hang case is what we actually want
            // to detect — panics are a separate (acceptable)
            // failure mode.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                swc_native::transform(source, &opts_bytes)
            }));
            let payload = match result {
                Ok(Ok(out)) => Ok(out.code),
                Ok(Err(e)) => Err(format!("transform error: {e}")),
                Err(p) => {
                    let msg = if let Some(s) = p.downcast_ref::<String>() {
                        s.clone()
                    } else if let Some(s) = p.downcast_ref::<&str>() {
                        (*s).to_string()
                    } else {
                        "panic with unknown payload".to_string()
                    };
                    Err(format!("panic: {msg}"))
                }
            };
            // Best-effort send — if the receiver is gone (test
            // already failed via timeout), we just discard.
            let _ = tx.send(payload);
        })
        .expect("spawn hang-fence worker");

    // Block up to HANG_FENCE waiting for the worker to deliver.
    // We deliberately do NOT join the worker on timeout — it may
    // be in an unkillable native syscall (the very condition this
    // test guards against). Letting the OS reap it on process exit
    // is the only correct response. Subsequent assertions in the
    // same test will short-circuit via the `?`-style propagation
    // in `assert_completes_in_fence` below.
    rx.recv_timeout(HANG_FENCE)
}

/// Wrapper that asserts the transform completed within the fence.
/// Panics with a HANG-DETECTED message on timeout (surfaces as a
/// failed test in standard cargo-test output).
fn assert_completes_in_fence(cwd: &Path, fixture: &'static str) {
    let path = cwd.join(fixture);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {fixture}: {e}"));
    let opts = build_opts_for(&path);

    let started = std::time::Instant::now();
    let outcome = transform_with_hang_fence(source, opts, fixture);
    let elapsed = started.elapsed();

    match outcome {
        Ok(Ok(_code)) => {
            // Folded or noop'd — either is fine. The parity harness
            // owns byte-equality coverage. We only care that the
            // transform reached a result in bounded time.
            eprintln!("[no-hang] {fixture}: ok in {elapsed:?}");
        }
        Ok(Err(err)) => {
            // Transform-level error (not a hang). Acceptable as
            // long as it's bounded — clean deopt errors are a
            // valid passing outcome for the dangling-symlink case.
            eprintln!("[no-hang] {fixture}: completed-with-error in {elapsed:?}: {err}");
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!(
                "HANG DETECTED on `{fixture}` — transform did not return \
                 within {fence:?} (this is the WASI symlink-canonicalisation \
                 hang re-emerging on native; check that `compat::wasi_path` \
                 still gates `relative_request_is_symlink` on \
                 `cfg(target_arch = \"wasm32\")` AND that the resolver's \
                 `symlinks` option flips to `true` on native — see \
                 `resolver/default.rs` and `resolver/engine.rs`).",
                fixture = fixture,
                fence = HANG_FENCE,
            );
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!(
                "no_hang_on_unreadable_imports: worker channel \
                 disconnected without delivering a result on `{fixture}` \
                 — worker likely panicked before send. This is a \
                 different bug from the hang we're guarding against; \
                 inspect the worker thread logs."
            );
        }
    }
}

#[test]
fn no_hang_on_unreadable_cross_file_imports() {
    // Stage the fixture layout under a unique tempdir per process so
    // parallel test runs / leftover dirs from prior runs don't
    // collide. `tempfile`-equivalent without the dependency: use the
    // process pid + a monotonic counter under `std::env::temp_dir()`.
    let tmp_root = std::env::temp_dir().join(format!(
        "swc_native_no_hang_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&tmp_root);
    std::fs::create_dir_all(&tmp_root).expect("create tmp_root");

    // Defer cleanup to a guard so even a panic'd test wipes the
    // tempdir on unwind.
    struct Cleanup<'a>(&'a Path);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0);
        }
    }
    let _cleanup = Cleanup(&tmp_root);

    let cwd = match stage_repro(&tmp_root).expect("stage_repro IO") {
        Some(c) => c,
        None => return, // self-skip — symlink creation not permitted.
    };

    // Run each fixture through the hang fence. Order matters only
    // weakly: if `input4` hangs, the test fails immediately and we
    // never reach 5/6/7 — but that's the right shape because the
    // first hang signal is the diagnostic the user needs.
    //
    // Regression baselines first (must be passing today):
    assert_completes_in_fence(&cwd, "input.tsx");
    assert_completes_in_fence(&cwd, "input3.tsx");
    assert_completes_in_fence(&cwd, "input7.tsx");
    // Hang reproducers — the actual regression guard:
    assert_completes_in_fence(&cwd, "input4.tsx");
    assert_completes_in_fence(&cwd, "input5.tsx");
    assert_completes_in_fence(&cwd, "input6.tsx");
}
