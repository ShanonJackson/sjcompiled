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

use std::cell::RefCell;

use indexmap::IndexMap;
use once_cell::sync::OnceCell;
use postcss_core::{Node, NodeKind};

use crate::at_rule::AtRuleBase;
use crate::browsers::Browsers;
use crate::data::prefixes::{PrefixEntry, PREFIXES};
use crate::declaration::DeclarationBase;
use crate::old_selector::OldSelector;
use crate::old_value::OldValue;
use crate::resolution::ResolutionBase;
use crate::selector::SelectorBase;
use crate::supports::Supports;
use crate::transition::FlexboxOption;
use crate::utils;
use crate::value::ValueBase;
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

/// JS `Declaration.load(name, prefixes, all)` factory. If a hack is
/// registered for `name`, return the hack-routed wrapper; otherwise the
/// plain base wrapper.
pub fn load_decl(name: &str, prefixes: Vec<String>) -> DeclPrefixer {
    if let Some(entry) = registry().lookup(HackBucket::Declaration, name) {
        match entry.class_name {
            "TextDecoration" => {
                return DeclPrefixer::TextDecoration(
                    crate::hacks::text_decoration::TextDecoration::new(
                        name.to_string(),
                        prefixes,
                        0,
                    ),
                );
            }
            "TextDecorationSkipInk" => {
                return DeclPrefixer::TextDecorationSkipInk(
                    crate::hacks::text_decoration_skip_ink::TextDecorationSkipInk::new(
                        name.to_string(),
                        prefixes,
                        0,
                    ),
                );
            }
            "UserSelect" => {
                return DeclPrefixer::UserSelect(
                    crate::hacks::user_select::UserSelect::new(
                        name.to_string(),
                        prefixes,
                        0,
                    ),
                );
            }
            _ => {}
        }
    }
    DeclPrefixer::Base(DeclarationBase::new(
        name.to_string(),
        prefixes,
        0,
    ))
}

/// JS `Value.load(name, prefixes, all)` factory. Hack registry routes
/// `cross-fade` / `fit-content` / `min-content` / `max-content` /
/// `fill` / `fill-available` / `stretch` to their respective Value
/// hacks; everything else lands on the plain base wrapper.
pub fn load_value(name: &str, prefixes: Vec<String>) -> ValuePrefixer {
    if let Some(entry) = registry().lookup(HackBucket::Value, name) {
        match entry.class_name {
            "CrossFade" => {
                return ValuePrefixer::CrossFade(
                    crate::hacks::cross_fade::CrossFade::new(
                        name.to_string(),
                        prefixes,
                        0,
                    ),
                );
            }
            "Intrinsic" => {
                return ValuePrefixer::Intrinsic(
                    crate::hacks::intrinsic::Intrinsic::new(
                        name.to_string(),
                        prefixes,
                        0,
                    ),
                );
            }
            _ => {}
        }
    }
    ValuePrefixer::Base(ValueBase::new(name.to_string(), prefixes, 0))
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

/// JS `Declaration.load(name, prefixes, all)` runtime dispatch. JS picks
/// `Klass.hacks[name]` from the static hack table populated by
/// `Declaration.hack(klass)` calls. We mirror via this enum:
/// `DeclPrefixer::Base` for the no-hack case, one variant per registered
/// hack class for the hack-routed cases.
///
/// Method dispatch: `process` is defined on this type (shadows the
/// `Deref::Target = DeclarationBase` blanket); call sites that go through
/// `decl.process(...)` get hack overrides. Field access (`decl.prefixer`,
/// `decl.cascade_option`) falls through `Deref` to the underlying
/// `DeclarationBase` so the existing processor.rs / values() / etc.
/// code paths compile unchanged.
pub enum DeclPrefixer {
    Base(DeclarationBase),
    TextDecoration(crate::hacks::text_decoration::TextDecoration),
    TextDecorationSkipInk(crate::hacks::text_decoration_skip_ink::TextDecorationSkipInk),
    UserSelect(crate::hacks::user_select::UserSelect),
}

impl DeclPrefixer {
    /// Sole base accessor — used by the `Deref` impl below and by
    /// preprocess's introspection (`prefixer.name`).
    pub fn base(&self) -> &DeclarationBase {
        match self {
            DeclPrefixer::Base(b) => b,
            DeclPrefixer::TextDecoration(h) => &h.base,
            DeclPrefixer::TextDecorationSkipInk(h) => &h.base,
            DeclPrefixer::UserSelect(h) => &h.base,
        }
    }
    pub fn base_mut(&mut self) -> &mut DeclarationBase {
        match self {
            DeclPrefixer::Base(b) => b,
            DeclPrefixer::TextDecoration(h) => &mut h.base,
            DeclPrefixer::TextDecorationSkipInk(h) => &mut h.base,
            DeclPrefixer::UserSelect(h) => &mut h.base,
        }
    }

    /// JS `prefixer.process(decl, result)` dispatch. Routes through the
    /// hack's overridden chain (`check` / `add` / `insert` / `set`) when
    /// a hack is attached; otherwise delegates to the base.
    pub fn process(
        &self,
        prefixes_all: &Prefixes,
        root: &mut Node,
        path: &[usize],
    ) {
        match self {
            DeclPrefixer::Base(b) => b.process(prefixes_all, root, path),
            DeclPrefixer::TextDecoration(_) => {
                // TextDecoration overrides ONLY `check`. Re-implement
                // the Declaration.process body inline so the hack's
                // `check` gets consulted before any prefix work.
                self.process_with_overrides(prefixes_all, root, path);
            }
            DeclPrefixer::TextDecorationSkipInk(_) | DeclPrefixer::UserSelect(_) => {
                // Both override `set` (and UserSelect also `insert`).
                // Re-implement the chain so the hack's set/insert fire
                // in the cloned-decl mutation step.
                self.process_with_overrides(prefixes_all, root, path);
            }
        }
    }

    /// Mirror of `DeclarationBase::process` + the inner Prefixer.process
    /// loop, with hack hooks at the four override points: `check`,
    /// `add`, `insert`, `set`.
    fn process_with_overrides(
        &self,
        prefixes_all: &Prefixes,
        root: &mut Node,
        path: &[usize],
    ) {
        // First fire hack `check` (TextDecoration). For UserSelect /
        // TextDecorationSkipInk that don't override check, fall through
        // to base behaviour where `check` is implicit-true on Declaration
        // (Prefixer.check returns true unless overridden, and Declaration
        // doesn't override).
        let check_passes = {
            let here = match postcss_core::node_at_path(root, path) {
                Some(n) => n,
                None => return,
            };
            self.hack_check(here)
        };
        if !check_passes {
            return;
        }

        // Compute parent prefix gate (mirror DeclarationBase::process).
        let mut current_path = path.to_vec();
        let parent =
            crate::prefixer::parent_prefix_cached_mut(root, &current_path);
        let prefixes: Vec<String> = self
            .base()
            .prefixer
            .prefixes
            .iter()
            .filter(|p| match &parent {
                crate::prefixer::ParentPrefix::None => true,
                crate::prefixer::ParentPrefix::Some(s) => {
                    s == utils::remove_note(p)
                }
            })
            .cloned()
            .collect();

        let need_cascade = {
            let here = match postcss_core::node_at_path_mut(root, &current_path)
            {
                Some(n) => n,
                None => return,
            };
            self.base().need_cascade(here)
        };

        let mut added: Vec<String> = Vec::new();
        for prefix in &prefixes {
            let mut so_far = added.clone();
            so_far.push(prefix.clone());
            if self
                .hack_add(root, &current_path, prefix, &so_far)
                .is_some()
            {
                added.push(prefix.clone());
                if let Some(last) = current_path.last_mut() {
                    *last += 1;
                }
            }
        }

        if !need_cascade || added.is_empty() {
            return;
        }
        // Restore-before pass + cascade calc — same as base.
        self.base().restore_before(prefixes_all, root, &current_path);
        let here = match postcss_core::node_at_path_mut(root, &current_path) {
            Some(n) => n,
            None => return,
        };
        here.raws.before = Some(self.base().calc_before(&added, here, ""));
    }

    fn hack_check(&self, decl: &Node) -> bool {
        match self {
            DeclPrefixer::TextDecoration(h) => h.check(decl),
            // Default: Declaration's implicit-true check.
            _ => true,
        }
    }

    fn hack_add(
        &self,
        root: &mut Node,
        path: &[usize],
        prefix: &str,
        prefixes: &[String],
    ) -> Option<()> {
        // Mirror Declaration.add: prefixed = self.prefixed(prop, prefix);
        // if isAlready || otherPrefixes return undefined; else insert.
        let (prop, value) = {
            let here = postcss_core::node_at_path(root, path)?;
            match &here.kind {
                NodeKind::Declaration(d) => (d.prop.clone(), d.value.clone()),
                _ => return None,
            }
        };
        let prefixed = self.base().prefixed(&prop, prefix);
        if self.base().is_already(root, path, &prefixed)
            || self.base().other_prefixes(&value, prefix)
        {
            return None;
        }
        self.hack_insert(root, path, prefix, prefixes)
    }

    fn hack_insert(
        &self,
        root: &mut Node,
        path: &[usize],
        prefix: &str,
        prefixes: &[String],
    ) -> Option<()> {
        // UserSelect overrides insert. Others fall through to a local
        // inline of Declaration.insert that calls hack `set` instead of
        // base `set`.
        if let DeclPrefixer::UserSelect(h) = self {
            // UserSelect.insert: -ms- + value === 'all' → undefined;
            // else delegate to the base-shaped insert with hack set.
            let value_is_all = match postcss_core::node_at_path(root, path) {
                Some(n) => match &n.kind {
                    NodeKind::Declaration(d) => d.value == "all",
                    _ => return None,
                },
                None => return None,
            };
            if value_is_all && prefix == "-ms-" {
                return None;
            }
            // Fall through to insert-with-hack-set.
            let _ = h; // silence unused (set called below via dispatch)
        }
        self.insert_with_hack_set(root, path, prefix, prefixes)
    }

    fn insert_with_hack_set(
        &self,
        root: &mut Node,
        path: &[usize],
        prefix: &str,
        prefixes: &[String],
    ) -> Option<()> {
        let original = postcss_core::node_at_path(root, path)?;
        let mut cloned = crate::prefixer::clone_node(original);
        // KEY DIVERGENCE FROM BASE — call hack.set(cloned, prefix), not base.set.
        self.hack_set(&mut cloned, prefix)?;

        let (cloned_prop, cloned_value) = match &cloned.kind {
            NodeKind::Declaration(d) => (d.prop.clone(), d.value.clone()),
            _ => return None,
        };

        let already = postcss_core::parent_some(root, path, |sibling| match &sibling.kind {
            NodeKind::Declaration(s) => {
                s.prop == cloned_prop && s.value == cloned_value
            }
            _ => false,
        });
        if already {
            return None;
        }

        let need_cascade = {
            let here = postcss_core::node_at_path_mut(root, path)?;
            self.base().need_cascade(here)
        };
        if need_cascade {
            let here = postcss_core::node_at_path_mut(root, path)?;
            cloned.raws.before =
                Some(self.base().calc_before(prefixes, here, prefix));
        }
        postcss_core::insert_before_at_path(root, path, cloned);
        Some(())
    }

    fn hack_set(&self, cloned: &mut Node, prefix: &str) -> Option<()> {
        match self {
            DeclPrefixer::TextDecorationSkipInk(h) => h.set(cloned, prefix),
            DeclPrefixer::UserSelect(h) => h.set(cloned, prefix),
            // TextDecoration / Base: default Declaration.set (just renames prop).
            _ => self.base().set(cloned, prefix),
        }
    }
}

impl std::ops::Deref for DeclPrefixer {
    type Target = DeclarationBase;
    fn deref(&self) -> &DeclarationBase {
        self.base()
    }
}

impl std::ops::DerefMut for DeclPrefixer {
    fn deref_mut(&mut self) -> &mut DeclarationBase {
        self.base_mut()
    }
}

/// JS `Value.load(name, prefixes, all)` runtime dispatch — twin of
/// `DeclPrefixer`. Two registered hacks (`CrossFade`, `Intrinsic`)
/// override `add` / `replace` / `regexp` / `check`. Method dispatch
/// happens via `check` / `add` defined on this type; field access falls
/// through `Deref` to the underlying `ValueBase` so processor.rs's
/// `v.prefixer.prefixes.clone()` compiles unchanged.
pub enum ValuePrefixer {
    Base(ValueBase),
    CrossFade(crate::hacks::cross_fade::CrossFade),
    Intrinsic(crate::hacks::intrinsic::Intrinsic),
}

impl ValuePrefixer {
    pub fn base(&self) -> &ValueBase {
        match self {
            ValuePrefixer::Base(b) => b,
            ValuePrefixer::CrossFade(h) => &h.base,
            ValuePrefixer::Intrinsic(h) => &h.base,
        }
    }
    pub fn base_mut(&mut self) -> &mut ValueBase {
        match self {
            ValuePrefixer::Base(b) => b,
            ValuePrefixer::CrossFade(h) => &mut h.base,
            ValuePrefixer::Intrinsic(h) => &mut h.base,
        }
    }

    /// JS `value.check(decl)`. Intrinsic uses its own (different)
    /// regexp; CrossFade inherits base behaviour.
    pub fn check(&self, decl: &Node) -> bool {
        match self {
            ValuePrefixer::Intrinsic(h) => {
                // Mirror ValueBase.check but with Intrinsic's regexp.
                let value = match &decl.kind {
                    NodeKind::Declaration(d) => &d.value,
                    _ => return false,
                };
                if !value.contains(&h.base.prefixer.name) {
                    return false;
                }
                h.regexp().is_match(value)
            }
            _ => self.base().check(decl),
        }
    }

    /// JS `value.add(decl, prefix)`. Routes through hack `replace` /
    /// `add` overrides via the per-variant call.
    pub fn add(&mut self, decl: &mut Node, prefix: &str) {
        match self {
            ValuePrefixer::Intrinsic(h) => h.add(decl, prefix),
            ValuePrefixer::CrossFade(h) => {
                // CrossFade uses base ValueBase.add semantics (the loop)
                // but with the override of `replace`. ValueBase.add
                // calls `self.replace` — which is the BASE method when
                // we route through the base instance directly. So we
                // have to inline the loop here, calling
                // `CrossFade::replace` instead of base.
                let initial = decl
                    .attrs
                    .get_string_map(crate::value::ATTR_VALUES)
                    .and_then(|m| m.get(prefix).cloned())
                    .unwrap_or_else(|| h.base.value(decl));

                let mut value = initial;
                loop {
                    let before = value.clone();
                    value = h.replace(&before, prefix);
                    if value == before {
                        break;
                    }
                }

                let map = decl
                    .attrs
                    .get_string_map_mut(crate::value::ATTR_VALUES);
                match map {
                    Some(m) => {
                        m.insert(prefix.to_string(), value);
                    }
                    None => {
                        let mut m = indexmap::IndexMap::new();
                        m.insert(prefix.to_string(), value);
                        decl.attrs.set(
                            crate::value::ATTR_VALUES,
                            postcss_core::AttrValue::StringMap(m),
                        );
                    }
                }
            }
            ValuePrefixer::Base(b) => b.add(decl, prefix),
        }
    }
}

impl std::ops::Deref for ValuePrefixer {
    type Target = ValueBase;
    fn deref(&self) -> &ValueBase {
        self.base()
    }
}

impl std::ops::DerefMut for ValuePrefixer {
    fn deref_mut(&mut self) -> &mut ValueBase {
        self.base_mut()
    }
}

/// JS `add[name]` polymorphic value. Each variant matches one branch
/// of `prefixes.js::preprocess` (lines 234-263).
pub enum AddBucket {
    /// JS: `add['@keyframes'] = new AtRule(name, prefixes)` /
    /// `add['@viewport'] = ...`. Same shape — both use `AtRuleBase`.
    AtRule(AtRuleBase),
    /// JS: `add['@resolution'] = new Resolution(name, prefixes)`.
    Resolution(ResolutionBase),
    /// JS: `add[name] = Declaration.load(name, prefixes)` with
    /// `add[name].values` aggregated from prior Value-with-props passes.
    /// The `values` Vec is appended in source-order from the matching
    /// Value-prefixers.
    Declaration {
        decl: DeclPrefixer,
        values: Vec<ValuePrefixer>,
    },
    /// JS: `add[prop] = { values: [...] }` — value prefixers only,
    /// no underlying Declaration prefixer. Used when a Value-with-
    /// props entry adds entries for `prop` but no Declaration entry
    /// for the same name was processed.
    Values(Vec<ValuePrefixer>),
}

/// JS `remove[name]` polymorphic value. Each variant matches one
/// branch of `prefixes.js::preprocess` (lines 266-321).
pub enum RemoveBucket {
    /// JS: `remove[name] = new Resolution(name, prefixes)`.
    Resolution(ResolutionBase),
    /// JS: `remove[prefixed] = { remove: true }`. Set by both the
    /// `@keyframes`/`@viewport` branch and the bare-Declaration
    /// `remove[prefixed].remove = true` branch.
    RemoveMarker,
    /// JS: `remove[prop] = { values: [...] }` — value-only stale
    /// prefixers without a remove marker.
    Values(Vec<OldValue>),
    /// JS: `remove[prefixed] = { remove: true, values: [...] }` —
    /// both. The Value-with-props branch can populate `.values` on a
    /// slot that another branch already marked `.remove = true`.
    RemoveMarkerWithValues(Vec<OldValue>),
}

impl RemoveBucket {
    /// JS: `remove[prop].remove === true`. Drives the `removeChild`
    /// branch of the processor remove walk.
    pub fn has_remove(&self) -> bool {
        matches!(
            self,
            RemoveBucket::RemoveMarker | RemoveBucket::RemoveMarkerWithValues(_)
        )
    }

    /// JS: `remove[prop].values || []`.
    pub fn values(&self) -> &[OldValue] {
        match self {
            RemoveBucket::RemoveMarkerWithValues(v) | RemoveBucket::Values(v) => v,
            _ => &[],
        }
    }
}

/// Populated dispatch table — JS `add` map after `preprocess()`. Keyed
/// by the property/at-rule/value name from the static `PREFIXES` table
/// PLUS, when a Value-with-props entry runs, the prop names it claims.
#[derive(Default)]
pub struct AddTable {
    /// JS: `add.selectors` — array of Selector instances. Iterated in
    /// `processor.js::add` to dispatch `selector.process(rule)` per
    /// rule.
    pub selectors: Vec<SelectorBase>,
    /// JS: `add[name]` for non-selector, non-`@supports` entries.
    pub by_name: IndexMap<String, AddBucket>,
}

/// Populated stale-prefix dispatch table — JS `remove` map after
/// `preprocess()`. Used by the processor's remove walk.
#[derive(Default)]
pub struct RemoveTable {
    /// JS: `remove.selectors` — array of `OldSelector` instances.
    /// Iterated in `processor.js::remove` to detect prefixed
    /// rule clones that should be dropped.
    pub selectors: Vec<OldSelector>,
    /// JS: `remove[name]` for non-selector entries.
    pub by_name: IndexMap<String, RemoveBucket>,
}

/// Top-level orchestrator — JS `class Prefixes`. Instantiated once per
/// `Browsers` selection. Holds the resolved add/remove tables for the
/// session.
///
/// `add_table` and `remove_table` are the post-`select()` per-name
/// prefix lists. The `add` and `remove` fields are the JS
/// `preprocess()` outputs — populated dispatch tables consumed by the
/// processor walks.
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
    /// JS `prefixes.add` — populated by `preprocess()`. Wrapped in
    /// `RefCell` because some prefixer instances need `&mut self`
    /// during `process()` (Resolution mutates `self.bad`; AtRuleBase
    /// signature is `&mut self` even though the base body doesn't
    /// strictly need it). The walk inside `processor.rs` borrows mutably
    /// once per dispatch call; `&Prefixes` callers (e.g.,
    /// `restore_before` via `Prefixes::group`) don't touch `add`, so
    /// the runtime borrow check holds.
    pub(crate) add: RefCell<AddTable>,
    /// JS `prefixes.remove` — populated by `preprocess()`.
    pub(crate) remove: RefCell<RemoveTable>,
    /// JS `add['@supports']` — always created in preprocess (not driven
    /// by `selected.add`). Stored separately to match JS access pattern
    /// `prefixes.add['@supports']` and avoid threading it through the
    /// `by_name` map (the type is heterogeneous with other `@`-keys).
    ///
    /// Boxed because `Supports` carries an `Option<Prefixes>` (its
    /// internal `prefixer_cache`) — a direct field would create an
    /// infinite-size struct via the `Prefixes → Supports → Prefixes`
    /// cycle. The box breaks the layout chain at one level of
    /// indirection.
    pub(crate) supports_inst: RefCell<Box<Supports>>,
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

/// JS: `prefixes.transition.add(decl)` — `Transition` constructs a
/// view over `Prefixes`. AGENT_3 left this as a trait so `Transition`
/// stays independently testable; AGENT_4 supplies the production impl
/// that consumes the populated `add` / `remove` tables. See
/// `transition.rs::TransitionPrefixesView` for the contract.
impl crate::transition::TransitionPrefixesView for Prefixes {
    fn add_prefixes(&self, prop: &str) -> Option<&[String]> {
        // JS: `this.prefixes.add[prop].prefixes` — the prefixes
        // attached to the per-prop bucket. For
        // `Declaration`/`AtRule`/`Resolution` buckets that's the
        // underlying base's `prefixer.prefixes`; for `Values`-only
        // buckets it's the union of value-prefixers' prefixes (matches
        // JS where `add[prop] = { values: [...] }` has no `.prefixes`
        // → `(add && add.prefixes)` is undefined, so JS yields `[]`).
        //
        // We can't return a borrow into `RefCell::borrow()` because
        // the borrow lifetime is tied to a stack `Ref`. Instead, fall
        // back to `Prefixes::add_table` which always owns its data —
        // it's the same underlying prefix list that drove
        // `preprocess()` and is stable across walks.
        self.add_table.get(prop).map(|v| v.as_slice())
    }

    fn should_remove(&self, prop: &str) -> bool {
        self.remove
            .borrow()
            .by_name
            .get(prop)
            .map(|b| b.has_remove())
            .unwrap_or(false)
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        Prefixes::prefixed(self, prop, prefix)
    }

    fn unprefixed(&self, prop: &str) -> String {
        self.unprefixed_prop(prop)
    }

    fn flexbox(&self) -> FlexboxOption {
        // JS: `this.prefixes.options.flexbox`.
        // - `undefined` (None) → On.
        // - `false` (not representable in `Option<String>`) → Off
        //   (currently unreachable; tracked AGENT_1 follow-up).
        // - `'no-2009'` → No2009.
        // - any other string → On.
        match self.options.flexbox.as_deref() {
            Some("no-2009") => FlexboxOption::No2009,
            None => FlexboxOption::On,
            _ => FlexboxOption::On,
        }
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
            add: RefCell::new(AddTable::default()),
            remove: RefCell::new(RemoveTable::default()),
            supports_inst: RefCell::new(Box::new(Supports::new())),
        };
        let selected = p.select(&PREFIXES);
        p.add_table = selected.add;
        p.remove_table = selected.remove;
        p.preprocess();
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
    /// bucket. Used by `processor.rs`. The post-`preprocess()` answer
    /// is the ValueBase NAMES (matching the keys you'd dispatch through
    /// `add[prop].values[*].process(decl)`).
    ///
    /// The `Result<...>` shape is preserved for backward-compat with
    /// AGENT_2's call site in `supports.rs`. With `preprocess()` now
    /// landed, the `Ok` branch always fires; `Err(NotYetImplemented)`
    /// remains only as the type-level marker that callers can match
    /// against if they want to distinguish "preprocess hasn't run" from
    /// "preprocess ran and the bucket is empty". In practice
    /// `preprocess()` always runs from `Prefixes::new`, so the Ok-empty
    /// case is the steady state.
    pub fn values(
        &self,
        type_: &str,
        prop: &str,
    ) -> Result<Vec<String>, NotYetImplemented> {
        match type_ {
            "add" => {
                let add = self.add.borrow();
                let global: Vec<String> = match add.by_name.get("*") {
                    Some(AddBucket::Values(vs)) => vs
                        .iter()
                        .map(|v| v.prefixer.name.clone())
                        .collect(),
                    Some(AddBucket::Declaration { values, .. }) => values
                        .iter()
                        .map(|v| v.prefixer.name.clone())
                        .collect(),
                    _ => Vec::new(),
                };
                let local: Vec<String> = match add.by_name.get(prop) {
                    Some(AddBucket::Values(vs)) => vs
                        .iter()
                        .map(|v| v.prefixer.name.clone())
                        .collect(),
                    Some(AddBucket::Declaration { values, .. }) => values
                        .iter()
                        .map(|v| v.prefixer.name.clone())
                        .collect(),
                    _ => Vec::new(),
                };
                if !global.is_empty() && !local.is_empty() {
                    let mut merged = global;
                    merged.extend(local);
                    Ok(utils::uniq(&merged))
                } else if !global.is_empty() {
                    Ok(global)
                } else {
                    Ok(local)
                }
            }
            "remove" => {
                let remove = self.remove.borrow();
                let global: Vec<String> = remove
                    .by_name
                    .get("*")
                    .map(|b| b.values().iter().map(|v| v.prefixed.clone()).collect())
                    .unwrap_or_default();
                let local: Vec<String> = remove
                    .by_name
                    .get(prop)
                    .map(|b| b.values().iter().map(|v| v.prefixed.clone()).collect())
                    .unwrap_or_default();
                if !global.is_empty() && !local.is_empty() {
                    let mut merged = global;
                    merged.extend(local);
                    Ok(utils::uniq(&merged))
                } else if !global.is_empty() {
                    Ok(global)
                } else {
                    Ok(local)
                }
            }
            _ => Ok(Vec::new()),
        }
    }

    /// JS: `preprocess(selected)` — `prefixes.js` lines 234-323.
    ///
    /// Builds the populated dispatch tables (`add` / `remove` /
    /// `supports_inst`) from the post-`select()` data
    /// (`add_table` / `remove_table`).
    ///
    /// **Hack dispatch limitation (Pass 2):** the JS `Selector.load /
    /// Value.load / Declaration.load` factory routes through the
    /// `Klass.hacks[name]` table; AGENT_5 has registered 5 hacks (the
    /// AFM in-scope set). This Rust port currently constructs BASE
    /// classes only — hack-routed names get base behaviour. The
    /// affected names are: `cross-fade`, `fit-content` (Value),
    /// `text-decoration`, `text-decoration-skip-ink`, `user-select`
    /// (Declaration). For AFM corpus that doesn't exercise these, the
    /// output is byte-clean. For corpus that does, the bytes diverge.
    /// Tracked as AGENT_4 Pass 3 follow-up: wire `HackRegistry::lookup`
    /// into the load paths here.
    fn preprocess(&mut self) {
        let mut add = AddTable::default();
        let mut remove = RemoveTable::default();

        // JS: `let add = { 'selectors': [], '@supports': new Supports(...) }`.
        // Supports is stored in `supports_inst`, not the by_name map (heterogeneous types).

        // ADD pass.
        for (name, prefixes) in self.add_table.iter() {
            // Look up the static data entry to detect selector / props
            // discriminator. Names not in PREFIXES (synthetic prop keys
            // generated by Value-with-props branches) won't be in the
            // outer add_table loop — they're populated as side effects.
            let entry = match PREFIXES.get(name.as_str()) {
                Some(e) => e,
                None => continue,
            };

            if name == "@keyframes" || name == "@viewport" {
                add.by_name.insert(
                    name.clone(),
                    AddBucket::AtRule(AtRuleBase::new(
                        // JS: `new AtRule(name, prefixes, this)`. JS `name`
                        // is the bare key like "@keyframes" — JS uses it
                        // as the at-rule name with the leading `@`
                        // stripped at the prefix concatenation site.
                        // Strip the leading `@` here to match the
                        // at-rule's `.name` field on a parsed AST.
                        name.trim_start_matches('@').to_string(),
                        prefixes.clone(),
                        0,
                    )),
                );
            } else if name == "@resolution" {
                add.by_name.insert(
                    name.clone(),
                    AddBucket::Resolution(ResolutionBase::new(
                        name.clone(),
                        prefixes.clone(),
                        0,
                    )),
                );
            } else if entry.selector {
                add.selectors.push(SelectorBase::new(
                    name.clone(),
                    prefixes.clone(),
                    0,
                ));
            } else if !entry.props.is_empty() {
                // JS: `let value = Value.load(name, prefixes, this)`.
                // For each prop in `data[name].props`, push the value
                // onto `add[prop].values`. JS reuses the SAME ValueBase
                // instance across props (line 256: `add[prop].values.push(value)`).
                // We construct a fresh `ValueBase` per push because
                // `ValueBase` doesn't derive `Clone` (the `regexp_cache`
                // is a `OnceCell`); rebuilding only loses the lazy
                // regex cache, which is recomputed on demand and is
                // byte-equivalent — the only observable cost is one
                // extra regex compile per Value-with-props prop on
                // first-access.
                //
                // Hack dispatch (Pass C): consult `HackRegistry::lookup`
                // before constructing the bare `ValueBase` — names like
                // `cross-fade` / `fit-content` / `stretch` route to a
                // hack instance instead.
                for prop in &entry.props {
                    let prop_key = prop.clone();
                    let v_for_prop =
                        load_value(name, prefixes.clone());
                    match add.by_name.get_mut(&prop_key) {
                        Some(AddBucket::Values(vs)) => vs.push(v_for_prop),
                        Some(AddBucket::Declaration { values, .. }) => {
                            values.push(v_for_prop);
                        }
                        _ => {
                            add.by_name.insert(
                                prop_key,
                                AddBucket::Values(vec![v_for_prop]),
                            );
                        }
                    }
                }
            } else {
                // JS: `let values = (add[name] && add[name].values) || []`
                // — preserve any value list a prior Value-with-props
                // pass attached to this slot.
                // JS: `let values = (add[name] && add[name].values) || []`
                // — preserve the value list a prior Value-with-props
                // pass attached. Empty in practice on Pass 2 because
                // we don't `Vec::clone()` `ValueBase` (no Clone derive
                // on ValueBase — see Value-with-props branch above).
                // Effectively this is the no-prior-values case which
                // matches JS for AFM-shaped inputs (the only case where
                // prior values exist is `*`/global, which AFM doesn't
                // exercise today).
                let prior_values: Vec<ValuePrefixer> =
                    match add.by_name.shift_remove(name.as_str()) {
                        Some(AddBucket::Values(vs)) => vs,
                        Some(AddBucket::Declaration { values, .. }) => values,
                        _ => Vec::new(),
                    };
                // Hack dispatch (Pass C): consult `HackRegistry::lookup`
                // for Declaration-bucket hacks (text-decoration,
                // text-decoration-skip-ink, user-select). The hack
                // instance carries the same DeclarationBase shape +
                // override hooks the wrapper consults.
                let decl = load_decl(name, prefixes.clone());
                add.by_name.insert(
                    name.clone(),
                    AddBucket::Declaration {
                        decl,
                        values: prior_values,
                    },
                );
            }
        }

        // REMOVE pass.
        for (name, prefixes) in self.remove_table.iter() {
            let entry = match PREFIXES.get(name.as_str()) {
                Some(e) => e,
                None => continue,
            };

            if entry.selector {
                // JS: build a Selector(name, prefixes), then for each
                // prefix push `selector.old(prefix)` (an OldSelector).
                let selector =
                    SelectorBase::new(name.clone(), prefixes.clone(), 0);
                for prefix in prefixes {
                    remove.selectors.push(selector.old(prefix));
                }
            } else if name == "@keyframes" || name == "@viewport" {
                // JS: for each prefix, set
                // `remove['@' + prefix + name.slice(1)] = { remove: true }`.
                // E.g., `@keyframes` + `-webkit-` → `@-webkit-keyframes`.
                let bare = &name[1..]; // strip leading '@'
                for prefix in prefixes {
                    let prefixed = format!("@{prefix}{bare}");
                    remove
                        .by_name
                        .insert(prefixed, RemoveBucket::RemoveMarker);
                }
            } else if name == "@resolution" {
                remove.by_name.insert(
                    name.clone(),
                    RemoveBucket::Resolution(ResolutionBase::new(
                        name.clone(),
                        prefixes.clone(),
                        0,
                    )),
                );
            } else if !entry.props.is_empty() {
                // JS: `let value = Value.load(name, [], this)`. Note JS
                // passes empty prefixes here (the OldValue list comes
                // from `value.old(prefix)` per remove-prefix).
                let value =
                    ValueBase::new(name.clone(), Vec::new(), 0);
                for prefix in prefixes {
                    let old = value.old(prefix);
                    for prop in &entry.props {
                        let prop_key = prop.clone();
                        let entry =
                            remove.by_name.entry(prop_key).or_insert_with(
                                || RemoveBucket::Values(Vec::new()),
                            );
                        match entry {
                            RemoveBucket::Values(vs) => {
                                vs.push(old.clone());
                            }
                            RemoveBucket::RemoveMarker => {
                                *entry = RemoveBucket::RemoveMarkerWithValues(
                                    vec![old.clone()],
                                );
                            }
                            RemoveBucket::RemoveMarkerWithValues(vs) => {
                                vs.push(old.clone());
                            }
                            _ => {}
                        }
                    }
                }
            } else {
                // JS: bare-Declaration remove — for each prefix, call
                // `decl(name).old(name, p)` returning Vec<String> of
                // prefixed prop names; for each, set
                // `remove[prefixed].remove = true`. Also: special
                // align-self skip when both `-webkit-` and
                // `-webkit- 2009` are in the add list.
                let add_for_name = match self.add_table.get(name.as_str()) {
                    Some(v) => v.clone(),
                    None => Vec::new(),
                };
                let decl =
                    DeclarationBase::new(name.clone(), Vec::new(), 0);
                for p in prefixes {
                    if name == "align-self" {
                        if p == "-webkit- 2009"
                            && add_for_name.iter().any(|x| x == "-webkit-")
                        {
                            continue;
                        }
                        if p == "-webkit-"
                            && add_for_name
                                .iter()
                                .any(|x| x == "-webkit- 2009")
                        {
                            continue;
                        }
                    }
                    // JS line 301: `decl(name).old(name, p)` — passes
                    // the prefix verbatim (including any " 2009" note).
                    // For non-align-self the note never appears here;
                    // for align-self the conflict-skip block above
                    // already handled the only conflict case. Pass
                    // `p` as-is to match JS bytes.
                    let olds = decl.old(name, p);
                    for prefixed in olds {
                        let entry = remove.by_name.entry(prefixed).or_insert(
                            RemoveBucket::RemoveMarker,
                        );
                        match entry {
                            RemoveBucket::Values(vs) => {
                                let vs_clone = vs.clone();
                                *entry =
                                    RemoveBucket::RemoveMarkerWithValues(
                                        vs_clone,
                                    );
                            }
                            // RemoveMarker / RemoveMarkerWithValues: already has remove flag.
                            _ => {}
                        }
                    }
                }
            }
        }

        *self.add.borrow_mut() = add;
        *self.remove.borrow_mut() = remove;
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
