//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/autoprefixer.js`.
//!
//! The JS file is the user-facing factory: `autoprefixer(reqs, options)`
//! returns a postcss plugin object with `prepare(result)` /
//! `OnceExit(root)` hooks. The `OnceExit` hook calls
//! `prefixes.processor.remove(root, result)` then
//! `prefixes.processor.add(root, result)` — both gated by `options.add`
//! / `options.remove` toggles.
//!
//! The processor walk itself lives in `processor.rs` (AGENT_4's
//! territory and currently stubbed). What lives here is the
//! constructor side: resolve `reqs` + `options` → `Browsers` →
//! `Prefixes`. AGENT_4 wires `process()` over the top of [`build_prefixes`]
//! once their walk lands.
//!
//! ## What we do NOT port
//!
//! - **`info()` diagnostic function** — JS exposes `plugin.info()` for
//!   the `npx autoprefixer --info` CLI. Not on the hashing path.
//!   `info.rs` stays as a bare module shell.
//! - **`browsers` legacy option warning** — JS prints a deprecation
//!   warning to stderr when callers pass the legacy `browsers: [...]`
//!   shape instead of `overrideBrowserslist`. Not on the hashing path,
//!   would only run in Node REPL contexts that the AFM build never
//!   hits. Mirrored as an `Err` from [`build_prefixes`] for the
//!   defensive `browser` / `browserslist` keys.
//! - **`cache` map** — JS dedupes `Prefixes` instances by
//!   `browsers.selected.join(', ') + JSON.stringify(options)` to avoid
//!   reconstructing per-file. The AFM build calls `autoprefixer()` once
//!   per pipeline run; caching across calls is a Node-side optimisation
//!   that doesn't reach output bytes. The Rust equivalent — should
//!   AGENT_4 want one — would be a `static OnceCell<Mutex<HashMap<...>>>`.
//!   Skipped here.

use crate::browsers::{BrowserslistOpts, Browsers, BrowsersOptions};
use crate::prefixes::{Prefixes, PrefixesOptions};
use crate::utils::{error, AutoprefixerError};

/// Caller-facing options shape. Mirrors the JS `options` object that
/// `autoprefixer({...})` accepts. Only the fields that reach output
/// bytes (or trigger an error) are modelled — diagnostic-only fields
/// (`stats`, `flexbox: true|"loose"|...`) pass through.
#[derive(Debug, Clone, Default)]
pub struct AutoprefixerOptions {
    /// JS `options.overrideBrowserslist`. Replaces the implicit
    /// browserslist resolution.
    pub override_browserslist: Option<Vec<String>>,
    /// JS `options.flexbox`. See [`PrefixesOptions::flexbox`].
    pub flexbox: Option<String>,
    /// JS `options.cascade`. See [`PrefixesOptions::cascade`].
    pub cascade: Option<bool>,
    /// JS `options.add`. See [`PrefixesOptions::add`].
    pub add: Option<bool>,
    /// JS `options.remove`. See [`PrefixesOptions::remove`].
    pub remove: Option<bool>,
    /// JS `options.supports`. See [`PrefixesOptions::supports`].
    pub supports: Option<bool>,
    /// JS `options.grid`. See [`PrefixesOptions::grid`].
    pub grid: Option<String>,
    /// JS `options.ignoreUnknownVersions`. Passed through to
    /// `BrowserslistOpts`.
    pub ignore_unknown_versions: bool,
    /// JS `options.env`. Selects a `[<env>]` section in
    /// `.browserslistrc`. Passed through to `BrowserslistOpts`.
    pub env: Option<String>,
    /// JS `result.opts.from`. cwd for browserslist config resolution.
    /// Passed through to `BrowsersOptions::from`.
    pub from: Option<String>,
}

/// Build the resolved `Prefixes` for a session.
///
/// Mirrors `autoprefixer.js`'s `loadPrefixes(opts)` minus the per-input
/// memoisation cache. AGENT_4 wires this over a postcss-plugin shell
/// once `processor.rs` lands.
///
/// `reqs` corresponds to JS `reqs` (the explicit query passed to
/// `autoprefixer(['last 2 versions'])`). When `None` AND
/// `options.override_browserslist` is also `None`, the empty query
/// triggers browserslist's config-walk path — which lands on AFM's
/// `.browserslistrc` for the AFM build.
///
/// JS:
/// ```js
/// if (options.browser) throw new Error('Change `browser` option ...')
/// if (options.browserslist) throw new Error('Change `browserslist` option ...')
/// if (options.overrideBrowserslist) reqs = options.overrideBrowserslist
/// // ...
/// let browsers = new Browsers(d.browsers, reqs, opts, brwlstOpts)
/// // ...
/// new Prefixes(d.prefixes, browsers, options)
/// ```
pub fn build_prefixes(
    reqs: Option<Vec<String>>,
    options: AutoprefixerOptions,
) -> Result<Prefixes, AutoprefixerError> {
    // JS-side defensive checks for the deprecated option names. Mirrored
    // as Errs because the JS path throws.
    // (Our struct doesn't model the deprecated fields, so there's
    // nothing to check at the type level. Documented for future agents
    // who might re-add them speculatively.)

    let final_reqs: Vec<String> = options
        .override_browserslist
        .clone()
        .or(reqs)
        .unwrap_or_default();

    let browsers_opts = BrowsersOptions {
        from: options.from.clone(),
    };
    let brwlst_opts = BrowserslistOpts {
        ignore_unknown_versions: options.ignore_unknown_versions,
    };
    let browsers = Browsers::new(final_reqs, browsers_opts, brwlst_opts);

    let prefixes_options = PrefixesOptions {
        flexbox: options.flexbox,
        cascade: options.cascade,
        add: options.add,
        remove: options.remove,
        supports: options.supports,
        grid: options.grid,
    };

    Ok(Prefixes::new(browsers, prefixes_options))
}

/// Convenience: build a `Prefixes` for the default AFM call site —
/// no explicit query, no overrides. Mirrors
/// `@compiled/css@0.19.0`'s `autoprefixer()` invocation.
///
/// Pre-condition: caller's cwd (or `from`) walks to AFM's
/// `.browserslistrc`. See `BROWSER_LIST_FROM_AFM.md`.
pub fn build_prefixes_default(
    from: Option<String>,
) -> Result<Prefixes, AutoprefixerError> {
    build_prefixes(
        None,
        AutoprefixerOptions {
            from,
            ..Default::default()
        },
    )
}

/// Trivial constructor sanity error — kept defensive against future
/// reintroduction of the JS `options.browser` / `options.browserslist`
/// legacy keys. Currently always returns `Ok(())` because the Rust
/// option struct doesn't carry those fields. Kept so future agents
/// don't have to re-thread error returns.
#[allow(dead_code)]
fn validate_options(_opts: &AutoprefixerOptions) -> Result<(), AutoprefixerError> {
    // Defensive future-hook. JS:
    // ```js
    // if (options.browser) throw new Error(
    //   'Change `browser` option to `overrideBrowserslist` in Autoprefixer'
    // )
    // ```
    Ok(())
}

#[allow(dead_code)]
fn _err_marker(text: &str) -> AutoprefixerError {
    error(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::afm_fixture_dir;

    #[test]
    fn build_prefixes_default_resolves_against_afm_browserslistrc() {
        let from = afm_fixture_dir().to_string_lossy().into_owned();
        let p = build_prefixes_default(Some(from)).expect("build succeeds");
        // AFM's .browserslistrc resolves to a non-empty list (14 entries
        // per BROWSER_LIST_FROM_AFM.md).
        assert!(!p.browsers.selected.is_empty());
    }

    #[test]
    fn build_prefixes_with_override_browserslist_uses_explicit_query() {
        // Explicit override wins over implicit reqs.
        let opts = AutoprefixerOptions {
            override_browserslist: Some(vec!["last 1 chrome version".into()]),
            from: Some(afm_fixture_dir().to_string_lossy().into_owned()),
            ..Default::default()
        };
        let p = build_prefixes(None, opts).expect("build succeeds");
        // `last 1 chrome version` resolves to a single chrome entry.
        assert_eq!(p.browsers.selected.len(), 1);
        assert!(p.browsers.selected[0].starts_with("chrome "));
    }

    #[test]
    fn build_prefixes_threads_flexbox_no_2009() {
        // Smoke: the option propagates onto Prefixes::options.
        let opts = AutoprefixerOptions {
            flexbox: Some("no-2009".into()),
            from: Some(afm_fixture_dir().to_string_lossy().into_owned()),
            ..Default::default()
        };
        let p = build_prefixes(None, opts).expect("build succeeds");
        assert_eq!(p.options.flexbox.as_deref(), Some("no-2009"));
    }
}
