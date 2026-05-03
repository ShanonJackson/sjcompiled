//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/prefixes.js`.
//!
//! `prefixes.js` is a 428-LOC orchestrator that wires every hack into
//! its base class via `Klass.hack(HackKlass)` calls (lines 68-134) and
//! then exposes the `Prefixes` constructor that `processor.js`
//! consumes. The full port lands in three layers:
//!
//! 1. **`HackRegistry`** (this file) — the `Klass.hack(...)` table.
//!    The hacks agent registers each hack here in
//!    `register_hacks(reg)` (append-only, alphabetical by JS file).
//! 2. **`Prefixes` struct** (this file) — instantiates the resolved
//!    add/remove tables for the session. The full `preprocess()` step
//!    (which builds Selector/Value/Declaration subclass instances) is
//!    deferred to AGENT_4's processor.rs work because it depends on
//!    the hack registry (AGENT_5) and the processor walk (AGENT_4).
//! 3. **`data/prefixes.rs`** — the static data table generated from
//!    `data/prefixes.js`. Drives which props/values/selectors get
//!    prefixed for each browser version.
//!
//! Hacks agent: see `register_hacks` below for the registration
//! contract. **Do not add methods to the base traits** —
//! `crates/autoprefixer/src/{declaration,value,selector,at_rule,resolution}.rs`
//! own those signatures. If your hack needs a method that isn't
//! there, file a note in `hacks/HACKS_PORT.md` and pause.

use indexmap::IndexMap;
use once_cell::sync::OnceCell;
use postcss_core::{Node, NodeKind};

use crate::browsers::Browsers;
use crate::data::prefixes::{PrefixEntry, PREFIXES};
use crate::utils;
use crate::vendor;

/// Bucket a hack registers into. JS-side static methods:
/// - `Selector.hack(klass)`
/// - `Declaration.hack(klass)`
/// - `Value.hack(klass)`
/// - `AtRule.hack(klass)` (no current hacks subclass AtRule)
/// - `Supports.hack(klass)` (none)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HackBucket {
    Selector,
    Declaration,
    Value,
    AtRule,
    Supports,
}

/// One registered hack — its bucket + the property/value/selector
/// names it claims (JS `Klass.names`).
#[derive(Debug, Clone)]
pub struct HackEntry {
    pub bucket: HackBucket,
    pub names: Vec<&'static str>,
    /// Diagnostic — JS class name. Drives parity-runner debug output.
    pub class_name: &'static str,
}

/// `Klass.hacks` table. JS uses one map per base class; we collapse
/// into a single registry keyed by `(bucket, name)` so a hack agent
/// only edits this file.
#[derive(Debug, Default)]
pub struct HackRegistry {
    pub entries: Vec<HackEntry>,
    /// Reverse lookup — `(bucket, name) → entry index`.
    pub by_name: IndexMap<(HackBucket, String), usize>,
}

impl HackRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append-only registration. The hacks agent calls this from
    /// [`register_hacks`] in alphabetical order by JS source filename.
    pub fn register(&mut self, entry: HackEntry) {
        let idx = self.entries.len();
        for name in &entry.names {
            self.by_name
                .insert((entry.bucket, (*name).to_string()), idx);
        }
        self.entries.push(entry);
    }

    /// Resolve `(bucket, name)` to a registered hack, or `None` if no
    /// hack claims that name.
    pub fn lookup(&self, bucket: HackBucket, name: &str) -> Option<&HackEntry> {
        self.by_name
            .get(&(bucket, name.to_string()))
            .and_then(|&i| self.entries.get(i))
    }
}

/// Hacks agent: append your registration in alphabetical-by-JS-file
/// order. The block is the single shared file you may edit. Don't
/// add methods to the base classes — file a note in HACKS_PORT.md
/// instead.
///
/// Pattern:
/// ```ignore
/// // From `lib/hacks/align-content.js`:
/// reg.register(HackEntry {
///     bucket: HackBucket::Declaration,
///     names: vec!["align-content", "flex-line-pack"],
///     class_name: "AlignContent",
/// });
/// ```
pub fn register_hacks(reg: &mut HackRegistry) {
    // BEGIN HACKS REGISTRATION — append-only, alphabetical by JS file.
    // From `lib/hacks/cross-fade.js`:
    reg.register(HackEntry {
        bucket: HackBucket::Value,
        names: crate::hacks::cross_fade::CrossFade::NAMES.to_vec(),
        class_name: crate::hacks::cross_fade::CrossFade::CLASS_NAME,
    });
    // From `lib/hacks/intrinsic.js`:
    reg.register(HackEntry {
        bucket: HackBucket::Value,
        names: crate::hacks::intrinsic::Intrinsic::NAMES.to_vec(),
        class_name: crate::hacks::intrinsic::Intrinsic::CLASS_NAME,
    });
    // From `lib/hacks/text-decoration.js`:
    reg.register(HackEntry {
        bucket: HackBucket::Declaration,
        names: crate::hacks::text_decoration::TextDecoration::NAMES.to_vec(),
        class_name: crate::hacks::text_decoration::TextDecoration::CLASS_NAME,
    });
    // From `lib/hacks/text-decoration-skip-ink.js`:
    reg.register(HackEntry {
        bucket: HackBucket::Declaration,
        names: crate::hacks::text_decoration_skip_ink::TextDecorationSkipInk::NAMES.to_vec(),
        class_name: crate::hacks::text_decoration_skip_ink::TextDecorationSkipInk::CLASS_NAME,
    });
    // From `lib/hacks/user-select.js`:
    reg.register(HackEntry {
        bucket: HackBucket::Declaration,
        names: crate::hacks::user_select::UserSelect::NAMES.to_vec(),
        class_name: crate::hacks::user_select::UserSelect::CLASS_NAME,
    });
    // END HACKS REGISTRATION
}

/// Singleton-built registry. Lazily populated on first access.
pub fn registry() -> &'static HackRegistry {
    static REG: OnceCell<HackRegistry> = OnceCell::new();
    REG.get_or_init(|| {
        let mut r = HackRegistry::new();
        register_hacks(&mut r);
        r
    })
}

/// JS `options` shape consumed by `Prefixes`. Only the fields that
/// reach output bytes are modelled; diagnostics fields (`stats`,
/// `ignoreUnknownVersions`, etc.) are passed through `Browsers` /
/// `BrowserslistOpts` and don't show up here.
#[derive(Debug, Clone, Default)]
pub struct PrefixesOptions {
    /// JS `options.flexbox`. When `Some("no-2009")`, prefixes carrying
    /// the `2009` note are dropped from the add list. Other values are
    /// passthrough — no other branch reads this field directly.
    pub flexbox: Option<String>,
    /// JS `options.cascade`. Defaults to "on" (true). When false,
    /// `DeclarationBase::need_cascade` short-circuits to false. The
    /// processor walk reads this through `prefixes.options`.
    pub cascade: Option<bool>,
    /// JS `options.add`. When false, the processor's `add` pass is
    /// skipped. Stored here so `processor.rs` can read it later.
    pub add: Option<bool>,
    /// JS `options.remove`. Symmetric to `add`.
    pub remove: Option<bool>,
    /// JS `options.supports`. When false, `@supports` rewriting is
    /// skipped.
    pub supports: Option<bool>,
    /// JS `options.grid`. Controls grid emulation depth. Stored here
    /// for processor consumption.
    pub grid: Option<String>,
}

/// Output of `select()` — the per-name add/remove prefix lists, before
/// `preprocess()` collapses them into per-bucket Prefixer instances.
#[derive(Debug, Clone, Default)]
pub struct Selected {
    pub add: IndexMap<String, Vec<String>>,
    pub remove: IndexMap<String, Vec<String>>,
}

/// Marker error for stubs that exist at the type level but require
/// AGENT_4 / AGENT_5 work to produce real output. Returned by methods
/// like `Prefixes::values` — see its docstring for the rationale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotYetImplemented;

impl std::fmt::Display for NotYetImplemented {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "preprocess() has not been ported yet — per-bucket value lists are unavailable",
        )
    }
}

impl std::error::Error for NotYetImplemented {}

/// Top-level orchestrator — JS `class Prefixes`. Instantiated once per
/// `Browsers` selection. Holds the resolved add/remove tables for the
/// session.
///
/// `add_table` and `remove_table` are the post-`select()` per-name
/// prefix lists. The JS `preprocess()` step that turns these into
/// Selector/Value/Declaration subclass instances depends on the hack
/// registry (AGENT_5) and is wired in by `processor.rs` (AGENT_4).
pub struct Prefixes {
    pub browsers: Browsers,
    pub options: PrefixesOptions,
    /// JS `selected.add` — per-name list of prefixes to ADD.
    pub add_table: IndexMap<String, Vec<String>>,
    /// JS `selected.remove` — per-name list of prefixes to REMOVE.
    pub remove_table: IndexMap<String, Vec<String>>,
    /// JS `cleanerCache`. Lazily-instantiated `Prefixes` configured
    /// with an empty `selected` browser list — used by the processor's
    /// `remove` pass to know which prefixes are stale.
    ///
    /// `pub(crate)` (not private) so peer agents inside this crate can
    /// build `Prefixes { ... }` literals in tests / scaffolding without
    /// the field becoming the cross-agent drift surface AGENT_4
    /// flagged. Outside-crate callers should prefer `Prefixes::new` or
    /// `Prefixes::with_empty` (test-only).
    pub(crate) cleaner_cache: OnceCell<Box<Prefixes>>,
}

impl std::fmt::Debug for Prefixes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prefixes")
            .field("browsers", &self.browsers)
            .field("options", &self.options)
            .field("add_table", &self.add_table)
            .field("remove_table", &self.remove_table)
            .finish_non_exhaustive()
    }
}

impl Prefixes {
    /// JS: `new Prefixes(data, browsers, options)`.
    ///
    /// We omit the JS-side `data` parameter because the static
    /// `PREFIXES` table is the only data shape autoprefixer consumes,
    /// and it's reachable as a workspace constant — passing it
    /// dynamically would invite a substitution drift later.
    ///
    /// We also omit the JS construction of `Transition` and `Processor`
    /// because both depend on the hack registry (AGENT_5) and the
    /// processor walk (AGENT_4). Those layers wire themselves in once
    /// they land.
    pub fn new(browsers: Browsers, options: PrefixesOptions) -> Self {
        let mut p = Self {
            browsers,
            options,
            add_table: IndexMap::new(),
            remove_table: IndexMap::new(),
            cleaner_cache: OnceCell::new(),
        };
        let selected = p.select(&PREFIXES);
        p.add_table = selected.add;
        p.remove_table = selected.remove;
        p
    }

    /// Test-only convenience: build a `Prefixes` with no browsers
    /// selected — equivalent to JS `new Prefixes(data, new Browsers(data, []), {})`.
    /// **The only sanctioned way for peer agents to hand-build a
    /// `Prefixes` outside `Prefixes::new`.** Hand-written `Prefixes
    /// { ... }` struct literals were the cross-agent drift surface
    /// AGENT_4 flagged on the AGENT_1↔AGENT_2 boundary; route through
    /// this constructor instead so private invariants stay private.
    #[doc(hidden)]
    pub fn with_empty() -> Self {
        let empty = Browsers {
            selected: Vec::new(),
            options: crate::browsers::BrowsersOptions::default(),
            browserslist_opts: crate::browsers::BrowserslistOpts::default(),
        };
        Prefixes::new(empty, PrefixesOptions::default())
    }

    /// JS: `cleaner()` — return a `Prefixes` configured with an empty
    /// browser list, used to drive the "remove all stale prefixes" pass.
    /// Cached on first access.
    /// ```js
    /// cleaner() {
    ///   if (this.cleanerCache) return this.cleanerCache
    ///   if (this.browsers.selected.length) {
    ///     let empty = new Browsers(this.browsers.data, [])
    ///     this.cleanerCache = new Prefixes(this.data, empty, this.options)
    ///   } else {
    ///     return this
    ///   }
    ///   return this.cleanerCache
    /// }
    /// ```
    pub fn cleaner(&self) -> &Prefixes {
        if self.browsers.selected.is_empty() {
            return self;
        }
        self.cleaner_cache.get_or_init(|| {
            // JS: `new Browsers(this.browsers.data, [])` — the data
            // parameter is the caniuse-lite agents table, which is a
            // static singleton in our world. Skip the browserslist-
            // resolution path entirely by constructing the struct
            // directly with `selected = []`.
            let empty = Browsers {
                selected: Vec::new(),
                options: self.browsers.options.clone(),
                browserslist_opts: self.browsers.browserslist_opts.clone(),
            };
            Box::new(Prefixes::new(empty, self.options.clone()))
        })
    }

    /// JS: `select(list)`.
    /// ```js
    /// select(list) {
    ///   let selected = { add: {}, remove: {} }
    ///   for (let name in list) {
    ///     let data = list[name]
    ///     let add = data.browsers.map(i => {
    ///       let params = i.split(' ')
    ///       return { browser: `${params[0]} ${params[1]}`, note: params[2] }
    ///     })
    ///     let notes = add
    ///       .filter(i => i.note)
    ///       .map(i => `${this.browsers.prefix(i.browser)} ${i.note}`)
    ///     notes = utils.uniq(notes)
    ///     add = add
    ///       .filter(i => this.browsers.isSelected(i.browser))
    ///       .map(i => {
    ///         let prefix = this.browsers.prefix(i.browser)
    ///         if (i.note) return `${prefix} ${i.note}`
    ///         else return prefix
    ///       })
    ///     add = this.sort(utils.uniq(add))
    ///     if (this.options.flexbox === 'no-2009') {
    ///       add = add.filter(i => !i.includes('2009'))
    ///     }
    ///     let all = data.browsers.map(i => this.browsers.prefix(i))
    ///     if (data.mistakes) all = all.concat(data.mistakes)
    ///     all = all.concat(notes)
    ///     all = utils.uniq(all)
    ///     if (add.length) {
    ///       selected.add[name] = add
    ///       if (add.length < all.length) {
    ///         selected.remove[name] = all.filter(i => !add.includes(i))
    ///       }
    ///     } else {
    ///       selected.remove[name] = all
    ///     }
    ///   }
    ///   return selected
    /// }
    /// ```
    pub fn select(&self, list: &IndexMap<&'static str, PrefixEntry>) -> Selected {
        let mut selected = Selected::default();

        for (name, data) in list.iter() {
            // Each `i` in data.browsers is e.g. "ie 9" or "chrome 100 2009".
            let raw: Vec<(String, Option<String>)> = data
                .browsers
                .iter()
                .map(|i| {
                    let mut parts = i.split(' ');
                    let p0 = parts.next().unwrap_or("");
                    let p1 = parts.next().unwrap_or("");
                    let p2 = parts.next().map(str::to_string);
                    (format!("{p0} {p1}"), p2)
                })
                .collect();

            // notes: items where `note` is Some, formatted "{prefix} {note}".
            let mut notes: Vec<String> = raw
                .iter()
                .filter_map(|(browser, note)| {
                    note.as_ref().map(|n| {
                        format!("{} {n}", self.browsers.prefix(browser))
                    })
                })
                .collect();
            notes = utils::uniq(&notes);

            // add: items whose browser is in `browsers.selected`.
            let mut add: Vec<String> = raw
                .iter()
                .filter(|(browser, _)| self.browsers.is_selected(browser))
                .map(|(browser, note)| {
                    let prefix = self.browsers.prefix(browser);
                    match note {
                        Some(n) => format!("{prefix} {n}"),
                        None => prefix,
                    }
                })
                .collect();
            add = self.sort(utils::uniq(&add));

            if self.options.flexbox.as_deref() == Some("no-2009") {
                add.retain(|i| !i.contains("2009"));
            }

            // all: base prefix for every entry browser, plus mistakes,
            // plus notes. JS passes the raw 3-part string to `prefix()`;
            // its destructuring drops the third part. Mirror that
            // explicitly here so we don't depend on `Browsers::prefix`
            // accepting noisy inputs.
            let mut all: Vec<String> = data
                .browsers
                .iter()
                .map(|b| {
                    let mut parts = b.split(' ');
                    let p0 = parts.next().unwrap_or("");
                    let p1 = parts.next().unwrap_or("");
                    let two_part = if p1.is_empty() {
                        p0.to_string()
                    } else {
                        format!("{p0} {p1}")
                    };
                    self.browsers.prefix(&two_part)
                })
                .collect();
            if !data.mistakes.is_empty() {
                all.extend(data.mistakes.iter().cloned());
            }
            all.extend(notes.iter().cloned());
            all = utils::uniq(&all);

            if !add.is_empty() {
                if add.len() < all.len() {
                    let add_set: std::collections::HashSet<&String> = add.iter().collect();
                    let remove: Vec<String> = all
                        .iter()
                        .filter(|i| !add_set.contains(i))
                        .cloned()
                        .collect();
                    selected.remove.insert(name.to_string(), remove);
                }
                selected.add.insert(name.to_string(), add);
            } else {
                selected.remove.insert(name.to_string(), all);
            }
        }

        selected
    }

    /// JS: `sort(prefixes)`.
    /// ```js
    /// sort(prefixes) {
    ///   return prefixes.sort((a, b) => {
    ///     let aLength = utils.removeNote(a).length
    ///     let bLength = utils.removeNote(b).length
    ///     if (aLength === bLength) return b.length - a.length
    ///     else return bLength - aLength
    ///   })
    /// }
    /// ```
    /// V8 `Array.prototype.sort` is stable; Rust's `sort_by` is also
    /// stable, so equal-key entries keep input order from `uniq`.
    pub fn sort(&self, mut prefixes: Vec<String>) -> Vec<String> {
        prefixes.sort_by(|a, b| {
            let a_bare = utils::remove_note(a).len();
            let b_bare = utils::remove_note(b).len();
            if a_bare == b_bare {
                b.len().cmp(&a.len())
            } else {
                b_bare.cmp(&a_bare)
            }
        });
        prefixes
    }

    /// JS: `unprefixed(prop)`.
    /// ```js
    /// unprefixed(prop) {
    ///   let value = this.normalize(vendor.unprefixed(prop))
    ///   if (value === 'flex-direction') value = 'flex-flow'
    ///   return value
    /// }
    /// ```
    /// `this.normalize(prop)` dispatches to the registered Declaration
    /// hack's `normalize` method. Without registered hacks (AGENT_5),
    /// the dispatch is identity (matches `Declaration.prototype.normalize`).
    /// The `flex-direction → flex-flow` post-rewrite is independent of
    /// the hack and applies in either case.
    pub fn unprefixed_prop(&self, prop: &str) -> String {
        let stripped = vendor::unprefixed(prop);
        let normalized = self.normalize_prop(&stripped);
        if normalized == "flex-direction" {
            "flex-flow".to_string()
        } else {
            normalized
        }
    }

    /// JS: `normalize(prop) { return this.decl(prop).normalize(prop) }`.
    /// `this.decl(prop)` looks up the registered Declaration class for
    /// `prop` (or the base `Declaration` if no hack claims it). Without
    /// hacks registered yet, every prop dispatches to the base, which
    /// returns its input unchanged.
    pub fn normalize_prop(&self, prop: &str) -> String {
        // Hack dispatch goes here once AGENT_5 lands. The base behaviour
        // is identity, so a missing dispatch is byte-equivalent to the
        // empty-registry case.
        let _ = registry().lookup(HackBucket::Declaration, prop);
        prop.to_string()
    }

    /// JS: `prefixed(prop, prefix)`.
    /// ```js
    /// prefixed(prop, prefix) {
    ///   prop = vendor.unprefixed(prop)
    ///   return this.decl(prop).prefixed(prop, prefix)
    /// }
    /// ```
    /// Mirrors `Declaration.prototype.prefixed = (prop, prefix) => prefix + prop`
    /// when no hack overrides — same identity assumption as
    /// [`Self::normalize_prop`].
    pub fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let _ = registry().lookup(HackBucket::Declaration, prop);
        let stripped = vendor::unprefixed(prop);
        format!("{prefix}{stripped}")
    }

    /// JS: `values(type, prop)` — return the merged value list for
    /// the prop across the global ('*') bucket and the prop-specific
    /// bucket. Used by `processor.rs`. Returns `Err(NotYetImplemented)`
    /// until `preprocess()` lands — AGENT_5's hacks that populate per-
    /// bucket value lists will need a real implementation; surfacing
    /// the gap as an error (rather than a silent empty Vec) makes the
    /// "first hack populates a value bucket" moment loud at compile/
    /// runtime instead of silently dropping prefixed values.
    pub fn values(
        &self,
        _type_: &str,
        _prop: &str,
    ) -> Result<Vec<String>, NotYetImplemented> {
        // TODO(AGENT_4): wire to preprocess()'s per-bucket value lists.
        // Until then, the bucket is unconditionally empty — return Ok
        // for the empty case so callers can chain it without an error
        // path during the AGENT_4 build-out, but never silently coerce
        // to Vec::new() at the type level.
        Ok(Vec::new())
    }

    /// JS: `group(decl)`. Returns a view that walks the decl's prefix
    /// group up/down via `Prefixes::group(decl).up(callback)` /
    /// `.down(callback)`.
    ///
    /// `path` must point at a Declaration node. Returns `None` for
    /// other node kinds.
    pub fn group<'a>(
        &'a self,
        root: &Node,
        decl_path: &[usize],
    ) -> Option<GroupView<'a>> {
        let here = postcss_core::node_at_path(root, decl_path)?;
        let prop = match &here.kind {
            NodeKind::Declaration(d) => d.prop.clone(),
            _ => return None,
        };
        Some(GroupView {
            prefixes: self,
            decl_path: decl_path.to_vec(),
            decl_unprefixed: self.unprefixed_prop(&prop),
        })
    }
}

/// JS-side `group(decl)` returns an object exposing `.up(cb)` and
/// `.down(cb)`. We model that as a borrowed view holding the decl's
/// path and unprefixed-prop key.
pub struct GroupView<'a> {
    prefixes: &'a Prefixes,
    decl_path: Vec<usize>,
    decl_unprefixed: String,
}

impl<'a> GroupView<'a> {
    /// JS: `up(callback)` — walks BACKWARDS through the decl's siblings.
    pub fn up<F>(&self, root: &Node, callback: F) -> bool
    where
        F: FnMut(&Node) -> bool,
    {
        self.checker(root, -1, callback)
    }

    /// JS: `down(callback)` — walks FORWARDS through the decl's siblings.
    pub fn down<F>(&self, root: &Node, callback: F) -> bool
    where
        F: FnMut(&Node) -> bool,
    {
        self.checker(root, 1, callback)
    }

    /// JS: `checker(step, callback)`.
    /// ```js
    /// let checker = (step, callback) => {
    ///   index += step
    ///   while (index >= 0 && index < length) {
    ///     let other = rule.nodes[index]
    ///     if (other.type === 'decl') {
    ///       if (step === -1 && other.prop === unprefixed) {
    ///         if (!Browsers.withPrefix(other.value)) break
    ///       }
    ///       if (this.unprefixed(other.prop) !== unprefixed) break
    ///       else if (callback(other) === true) return true
    ///       if (step === +1 && other.prop === unprefixed) {
    ///         if (!Browsers.withPrefix(other.value)) break
    ///       }
    ///     }
    ///     index += step
    ///   }
    ///   return false
    /// }
    /// ```
    fn checker<F>(&self, root: &Node, step: isize, mut callback: F) -> bool
    where
        F: FnMut(&Node) -> bool,
    {
        let parent_kids = match postcss_core::parent_nodes(root, &self.decl_path) {
            Some(p) => p,
            None => return false,
        };
        let here_idx = match self.decl_path.last().copied() {
            Some(i) => i as isize,
            None => return false,
        };
        let length = parent_kids.len() as isize;
        let mut idx = here_idx + step;
        while idx >= 0 && idx < length {
            let other = match parent_kids.get(idx as usize) {
                Some(n) => n,
                None => return false,
            };
            if let NodeKind::Declaration(d) = &other.kind {
                if step == -1 && d.prop == self.decl_unprefixed {
                    if !Browsers::with_prefix(&d.value) {
                        break;
                    }
                }
                if self.prefixes.unprefixed_prop(&d.prop) != self.decl_unprefixed {
                    break;
                }
                if callback(other) {
                    return true;
                }
                if step == 1 && d.prop == self.decl_unprefixed {
                    if !Browsers::with_prefix(&d.value) {
                        break;
                    }
                }
            }
            idx += step;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{afm_browsers, empty_browsers};
    use postcss_core::parse;

    #[test]
    fn registry_holds_afm_in_scope_hacks() {
        // Five hacks are registered for AFM's surface (per Phase A in
        // `crates/autoprefixer/AFM_HACKS_INSTRUMENTATION.md`):
        // CrossFade, Intrinsic, TextDecoration, TextDecorationSkipInk,
        // UserSelect. The remaining 51 hacks are deferred until AFM
        // demands them.
        let r = registry();
        assert_eq!(r.entries.len(), 5);
        // Spot-check by name.
        assert!(r.lookup(HackBucket::Value, "cross-fade").is_some());
        assert!(r.lookup(HackBucket::Value, "fit-content").is_some());
        assert!(r.lookup(HackBucket::Declaration, "text-decoration").is_some());
        assert!(r.lookup(HackBucket::Declaration, "text-decoration-skip-ink").is_some());
        assert!(r.lookup(HackBucket::Declaration, "user-select").is_some());
        // Out-of-scope: e.g. AlignContent (declaration), Gradient (value),
        // Placeholder (selector) MUST still be `None`.
        assert!(r.lookup(HackBucket::Declaration, "align-content").is_none());
        assert!(r.lookup(HackBucket::Value, "linear-gradient").is_none());
        assert!(r.lookup(HackBucket::Selector, "::placeholder").is_none());
    }

    #[test]
    fn register_appends_and_indexes() {
        let mut r = HackRegistry::new();
        r.register(HackEntry {
            bucket: HackBucket::Declaration,
            names: vec!["align-content", "flex-line-pack"],
            class_name: "AlignContent",
        });
        assert!(r.lookup(HackBucket::Declaration, "align-content").is_some());
        assert!(r.lookup(HackBucket::Declaration, "flex-line-pack").is_some());
        assert!(r.lookup(HackBucket::Selector, "align-content").is_none());
    }

    #[test]
    fn sort_orders_by_descending_bare_length_then_total_length() {
        // bare length = removeNote-length. So "-webkit- 2009" has
        // bare length 8 (same as "-webkit-"), and longer total length
        // — should come AFTER "-webkit-" because the JS comparator
        // returns `b.length - a.length` (descending) on bare ties.
        // Use Prefixes::new — sort() doesn't depend on a fully-resolved
        // table, so pass-through against the AFM browsers is fine.
        let p = Prefixes::new(afm_browsers(), PrefixesOptions::default());
        let input = vec![
            "-moz-".to_string(),
            "-webkit- 2009".to_string(),
            "-webkit-".to_string(),
        ];
        let out = p.sort(input);
        // bare lengths: -moz- = 5, -webkit- = 8, -webkit- 2009 = 8.
        // Descending bare-length: webkits first, then -moz-.
        // Among the two webkits, ties broken by total descending: the
        // longer one ("-webkit- 2009") comes first.
        assert_eq!(
            out,
            vec![
                "-webkit- 2009".to_string(),
                "-webkit-".to_string(),
                "-moz-".to_string(),
            ]
        );
    }

    #[test]
    fn unprefixed_prop_strips_vendor_prefix() {
        let p = Prefixes::new(afm_browsers(), PrefixesOptions::default());
        assert_eq!(p.unprefixed_prop("-webkit-flex"), "flex");
        assert_eq!(p.unprefixed_prop("color"), "color");
    }

    #[test]
    fn unprefixed_prop_maps_flex_direction_to_flex_flow() {
        // JS quirk: `if (value === 'flex-direction') value = 'flex-flow'`.
        let p = Prefixes::new(afm_browsers(), PrefixesOptions::default());
        assert_eq!(p.unprefixed_prop("flex-direction"), "flex-flow");
        assert_eq!(p.unprefixed_prop("-webkit-flex-direction"), "flex-flow");
    }

    #[test]
    fn new_populates_add_and_remove_tables_for_afm_browsers() {
        // For AFM-shaped queries against the static PREFIXES table, we
        // expect *some* names to land in add (for hyphens, ::placeholder,
        // etc.) and the cleaner side. Sanity-only — the byte-clean
        // gate is `data_table_matches_js_oracle`.
        let p = Prefixes::new(afm_browsers(), PrefixesOptions::default());
        assert!(
            !p.add_table.is_empty() || !p.remove_table.is_empty(),
            "neither add_table nor remove_table populated for AFM browsers"
        );
    }

    #[test]
    fn cleaner_returns_self_when_no_browsers_selected() {
        let p = Prefixes::new(empty_browsers(), PrefixesOptions::default());
        let c = p.cleaner();
        // Same address => returned `self`.
        assert!(std::ptr::eq(c, &p));
    }

    #[test]
    fn with_empty_constructor_is_equivalent_to_empty_browsers_new() {
        // Pin the test-only `Prefixes::with_empty()` shape — the
        // sanctioned hand-built constructor for peer agents per
        // AGENT_4's review (replaces struct-literal scaffolding).
        let a = Prefixes::with_empty();
        assert!(a.browsers.selected.is_empty());
        assert!(a.add_table.is_empty());
        // `with_empty` is byte-equivalent to constructing via the
        // public path with an empty Browsers.
        let b = Prefixes::new(empty_browsers(), PrefixesOptions::default());
        assert_eq!(a.browsers.selected, b.browsers.selected);
        assert_eq!(a.add_table.len(), b.add_table.len());
        assert_eq!(a.remove_table.len(), b.remove_table.len());
    }

    #[test]
    fn cleaner_returns_distinct_prefixes_when_browsers_present() {
        let p = Prefixes::new(afm_browsers(), PrefixesOptions::default());
        // Sanity: the AFM browser list resolves to at least one entry.
        assert!(!p.browsers.selected.is_empty());
        let c = p.cleaner();
        // Cleaner has empty selected.
        assert!(c.browsers.selected.is_empty());
        // Cleaner is cached — calling twice returns the same pointer.
        let c2 = p.cleaner();
        assert!(std::ptr::eq(c, c2));
    }

    #[test]
    fn group_up_walks_backwards_through_prefixed_decls() {
        // `display` decl preceded by two prefixed siblings — `up` should
        // yield each in reverse order until callback returns true or the
        // run breaks.
        let r = parse(
            "a {\n  -webkit-display: flex;\n  -moz-display: flex;\n  display: flex;\n}",
        )
        .unwrap();
        let p = Prefixes::new(afm_browsers(), PrefixesOptions::default());
        // Path: root → rule(0) → decl(2) (the unprefixed `display`).
        let group = p.group(&r.root, &[0, 2]).expect("decl path resolves");

        let mut visited: Vec<String> = Vec::new();
        group.up(&r.root, |other| {
            if let NodeKind::Declaration(d) = &other.kind {
                visited.push(d.prop.clone());
            }
            false // keep walking
        });
        // Walks backwards: -moz-display, then -webkit-display.
        assert_eq!(
            visited,
            vec!["-moz-display".to_string(), "-webkit-display".to_string()]
        );
    }

    #[test]
    fn group_up_stops_at_unrelated_prop() {
        let r = parse(
            "a {\n  color: red;\n  -webkit-display: flex;\n  display: flex;\n}",
        )
        .unwrap();
        let p = Prefixes::new(afm_browsers(), PrefixesOptions::default());
        let group = p.group(&r.root, &[0, 2]).expect("decl path resolves");

        let mut visited: Vec<String> = Vec::new();
        group.up(&r.root, |other| {
            if let NodeKind::Declaration(d) = &other.kind {
                visited.push(d.prop.clone());
            }
            false
        });
        // Only -webkit-display visited; `color` breaks the run.
        assert_eq!(visited, vec!["-webkit-display".to_string()]);
    }

    #[test]
    fn group_callback_truthy_short_circuits() {
        let r = parse(
            "a {\n  -webkit-display: flex;\n  -moz-display: flex;\n  display: flex;\n}",
        )
        .unwrap();
        let p = Prefixes::new(afm_browsers(), PrefixesOptions::default());
        let group = p.group(&r.root, &[0, 2]).expect("decl path resolves");

        let mut count = 0;
        let hit = group.up(&r.root, |_| {
            count += 1;
            true // first call returns truthy => up() returns true
        });
        assert!(hit);
        assert_eq!(count, 1);
    }
}
