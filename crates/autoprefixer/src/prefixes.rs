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
//! 2. **`Prefixes` struct** (this file, currently `unimplemented!()` for
//!    the heavy methods) — instantiates one Prefixer per declared
//!    `name`, exposes `add` / `remove` / `process` / `prefixed`
//!    against the parsed AST.
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
pub fn register_hacks(_reg: &mut HackRegistry) {
    // BEGIN HACKS REGISTRATION — append-only, alphabetical by JS file.
    // (Empty until first hack lands.)
    // END HACKS REGISTRATION
}

/// Singleton-built registry. Lazily populated on first access.
pub fn registry() -> &'static HackRegistry {
    use once_cell::sync::OnceCell;
    static REG: OnceCell<HackRegistry> = OnceCell::new();
    REG.get_or_init(|| {
        let mut r = HackRegistry::new();
        register_hacks(&mut r);
        r
    })
}

/// Top-level orchestrator — JS `class Prefixes`. Instantiated once per
/// `Browsers` selection. Holds the resolved add/remove tables for the
/// session.
///
/// Most methods are unimplemented pending `data/prefixes.rs` (the
/// caniuse-derived static data table) and `processor.rs`. Body
/// signatures are locked here so the hacks agent + processor port can
/// be written against the final shape.
#[derive(Debug)]
pub struct Prefixes {
    pub browsers: crate::browsers::Browsers,
    /// Data subset for the selected browser list.
    pub add_table: IndexMap<String, Vec<String>>,
    pub remove_table: IndexMap<String, Vec<String>>,
}

impl Prefixes {
    /// JS: `new Prefixes(data, browsers, options)`.
    pub fn new(_browsers: crate::browsers::Browsers) -> Self {
        unimplemented!(
            "Phase 7 — port prefixes.js constructor; depends on data/prefixes.rs"
        )
    }

    /// JS: `cleaner()` — returns a `Prefixes` configured for removal
    /// of stale prefixes. We model this as a lazy field on the same
    /// struct in the port.
    pub fn cleaner(&self) -> &Prefixes {
        unimplemented!("Phase 7 — port prefixes.js::cleaner")
    }

    /// JS: `select(list)` — pick add/remove targets.
    pub fn select(&mut self, _list: &IndexMap<String, IndexMap<String, Vec<String>>>) {
        unimplemented!("Phase 7 — port prefixes.js::select")
    }

    /// JS: `group(node)` — return a "group" view of declarations
    /// adjacent to `node`. Used by `declaration.js::restoreBefore` and
    /// `declaration.js::isAlready`.
    pub fn group(&self, _node: &postcss_core::Node) {
        unimplemented!("Phase 7 — port prefixes.js::group")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_initially_empty() {
        let r = registry();
        assert_eq!(r.entries.len(), 0);
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
}
