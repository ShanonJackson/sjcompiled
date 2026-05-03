//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/supports.js`.
//!
//! Rewrites `@supports` rule preludes so that prefixed-feature variants
//! are emitted alongside unprefixed clauses, joined by ` or `. The full
//! pipeline mirrors JS `Supports::process`:
//!
//!   `brackets.parse(rule.params)`
//!     → `normalize`
//!     → `remove`
//!     → `add`
//!     → `cleanBrackets`
//!     → `brackets.stringify`
//!
//! Status of each method against the AGENT_1-landed `Prefixes` shape:
//!
//! | JS                | Rust                | Body         | Notes |
//! |-------------------|---------------------|--------------|-------|
//! | `parse`           | `parse`             | byte-clean   |       |
//! | `isNot`           | `is_not`            | byte-clean   | bug-for-bug regex (no anchors) |
//! | `isOr`            | `is_or`             | byte-clean   | bug-for-bug regex (no anchors) |
//! | `isProp`          | `is_prop`           | byte-clean   |       |
//! | `isHack`          | `is_hack`           | byte-clean   |       |
//! | `cleanBrackets`   | `clean_brackets`    | byte-clean   |       |
//! | `convert`         | `convert`           | byte-clean   |       |
//! | `normalize`       | `normalize`         | byte-clean   |       |
//! | `virtual`         | `virtual_rule`      | byte-clean   | `virtual` is a Rust keyword |
//! | `prefixer`        | `prefixer`          | wired        | uses `Prefixes::new` (AGENT_1) |
//! | `prefixed`        | `prefixed`          | partial      | inner Prefixer/Value calls are no-ops until `preprocess()` lands (AGENT_4); meanwhile returns the bare virtual rule (matches JS for empty add-table) |
//! | `toRemove`        | `to_remove`         | partial      | returns `false` until `cleaner` exposes `.remove[prop].remove` markers + `values('remove')` (AGENT_4); JS returns `false` in the same empty-preprocess state |
//! | `remove`          | `remove`            | byte-clean   | calls `to_remove` |
//! | `add`             | `add_prefixes`      | byte-clean   | calls `prefixed` |
//! | `process`         | `process`           | byte-clean   | end-to-end pipeline orchestrator |
//! | `disabled`        | `disabled`          | partial      | `options.flexbox` is `Option<String>` in `PrefixesOptions`, can't represent JS `=== false` distinctly; flexbox branch never fires today (note in AGENT_2_DONE) |
//!
//! The static `SUPPORTED` list (line 12 of supports.js — browsers/versions
//! that support `@supports` at all) is loaded eagerly from the
//! `css-featurequeries` feature in the frozen caniuse-lite snapshot.

use once_cell::sync::Lazy;
use regex::Regex;

use postcss_core::{Node as CoreNode, NodeKind};

use crate::brackets::{self, Node as BracketNode};
use crate::browsers::Browsers;
use crate::prefixes::Prefixes;
use crate::utils;

/// JS:
/// ```js
/// let featureQueries = require('caniuse-lite/data/features/css-featurequeries.js')
/// let feature = require('caniuse-lite/dist/unpacker/feature')
/// let data = feature(featureQueries)
/// let supported = []
/// for (let browser in data.stats) {
///   for (let version in data.stats[browser]) {
///     if (/y/.test(data.stats[browser][version])) supported.push(browser + ' ' + version)
///   }
/// }
/// ```
///
/// Iteration order matches JS — `IndexMap` preserves insertion order
/// for both the outer (browser) and inner (version) keys.
pub static SUPPORTED: Lazy<Vec<String>> = Lazy::new(|| {
    let mut out: Vec<String> = Vec::new();
    let feat = match caniuse_db::features::feature("css-featurequeries") {
        Some(f) => f,
        None => return out,
    };
    for (browser, versions) in &feat.stats {
        for (version, support) in versions {
            // `/y/.test(support)` — any 'y' anywhere (covers `y`, `y x`,
            // `a y`, etc.). Single-char `contains` matches.
            if support.contains('y') {
                out.push(format!("{browser} {version}"));
            }
        }
    }
    out
});

/// JS `class Supports`. Holds the lazily-built sub-`Prefixes` cache
/// (`prefixerCache` in JS). Constructed by `Prefixes::preprocess` as
/// `add['@supports'] = new Supports(Prefixes, this)`.
///
/// The JS constructor takes `(Prefixes_class, all_instance)`. In Rust we
/// don't pass classes around — `Prefixes::new` is a free function — so
/// we drop the first arg. The `all` reference is passed on each method
/// call rather than stored, so `Supports` is independently testable and
/// avoids a self-referential lifetime through `Prefixes`.
pub struct Supports {
    /// Mirrors JS `this.prefixerCache`. Built on first call to `prefixer()`.
    prefixer_cache: Option<Prefixes>,
}

impl Default for Supports {
    fn default() -> Self {
        Self::new()
    }
}

impl Supports {
    /// JS: `constructor(Prefixes, all)`.
    pub fn new() -> Self {
        Self { prefixer_cache: None }
    }

    /// JS: `prefixer()`.
    /// ```js
    /// prefixer() {
    ///   if (this.prefixerCache) return this.prefixerCache
    ///   let filtered = this.all.browsers.selected.filter(i => supported.includes(i))
    ///   let browsers = new Browsers(this.all.browsers.data, filtered, this.all.options)
    ///   this.prefixerCache = new this.Prefixes(this.all.data, browsers, this.all.options)
    ///   return this.prefixerCache
    /// }
    /// ```
    ///
    /// Filters `all.browsers.selected` to those that support `@supports`,
    /// then constructs a sub-`Prefixes` keyed by that filtered list. Used
    /// by `prefixed(str)` so the synthesized declarations in the virtual
    /// rule only get prefixes that the `@supports`-supporting browsers
    /// would understand.
    pub fn prefixer(&mut self, all: &Prefixes) -> &Prefixes {
        if self.prefixer_cache.is_none() {
            let filtered: Vec<String> = all
                .browsers
                .selected
                .iter()
                .filter(|b| SUPPORTED.iter().any(|s| s == *b))
                .cloned()
                .collect();
            // Construct the filtered Browsers without going back through
            // browserslist resolution — JS hand-builds it here too
            // (`new Browsers(this.all.browsers.data, filtered, this.all.options)`),
            // bypassing the parser path because `filtered` is already a
            // resolved list of "name version" strings.
            let sub_browsers = Browsers {
                selected: filtered,
                options: all.browsers.options.clone(),
                browserslist_opts: all.browsers.browserslist_opts.clone(),
            };
            self.prefixer_cache =
                Some(Prefixes::new(sub_browsers, all.options.clone()));
        }
        self.prefixer_cache.as_ref().unwrap()
    }

    /// JS: `parse(str)`.
    /// ```js
    /// parse(str) {
    ///   let parts = str.split(':')
    ///   let prop = parts[0]
    ///   let value = parts[1]
    ///   if (!value) value = ''
    ///   return [prop.trim(), value.trim()]
    /// }
    /// ```
    ///
    /// Splits on every `:`, takes positions [0] and [1] only — a third
    /// `:` is silently dropped (JS `parts[1]` is just the second piece,
    /// not the rest). `nth(1)` matches.
    pub fn parse(&self, s: &str) -> (String, String) {
        let mut iter = s.split(':');
        let prop = iter.next().unwrap_or("");
        let value = iter.next().unwrap_or("");
        (prop.trim().to_string(), value.trim().to_string())
    }

    /// JS: `virtual(str)`.
    /// ```js
    /// virtual(str) {
    ///   let [prop, value] = this.parse(str)
    ///   let rule = parse('a{}').first
    ///   rule.append({ prop, value, raws: { before: '' } })
    ///   return rule
    /// }
    /// ```
    ///
    /// Returns a fresh Rule node (`a {}`) with one Declaration child.
    /// `raws.before = ''` is load-bearing — `Value.save` reads it.
    /// `virtual` is a Rust keyword, hence `virtual_rule`.
    pub fn virtual_rule(&self, s: &str) -> CoreNode {
        let (prop, value) = self.parse(s);
        let mut root = postcss_core::parse("a{}").expect("parse('a{}') always succeeds");
        let mut rule = root
            .root
            .nodes_mut()
            .expect("root has nodes")
            .remove(0);
        let mut decl_node = CoreNode::new(NodeKind::Declaration(
            postcss_core::declaration::Declaration {
                prop,
                value,
                important: false,
                variable: false,
            },
        ));
        decl_node.raws.before = Some(String::new());
        rule.nodes_mut()
            .expect("rule has block")
            .push(decl_node);
        rule
    }

    /// JS: `prefixed(str)`.
    /// ```js
    /// prefixed(str) {
    ///   let rule = this.virtual(str)
    ///   if (this.disabled(rule.first)) return rule.nodes
    ///   let result = { warn: () => null }
    ///   let prefixer = this.prefixer().add[rule.first.prop]
    ///   prefixer && prefixer.process && prefixer.process(rule.first, result)
    ///   for (let decl of rule.nodes) {
    ///     for (let value of this.prefixer().values('add', rule.first.prop)) {
    ///       value.process(decl)
    ///     }
    ///     Value.save(this.all, decl)
    ///   }
    ///   return rule.nodes
    /// }
    /// ```
    ///
    /// Returns the list of declarations in the virtual rule after the
    /// prefix-add pass. With AGENT_4's `preprocess()` not yet wired, the
    /// inner `prefixer.process` and `Value::save` calls are no-ops — the
    /// function returns the single virtual decl as-is. Once `preprocess`
    /// lands, the bracketed `prefixer && prefixer.process` and value-loop
    /// branches will start firing automatically because `add_table` /
    /// `values()` will be populated.
    pub fn prefixed(&mut self, s: &str, all: &Prefixes) -> Vec<CoreNode> {
        let mut rule = self.virtual_rule(s);
        let first = rule
            .nodes()
            .and_then(|n| n.first())
            .cloned()
            .expect("virtual_rule produces a rule with one decl");
        if self.disabled(&first, all) {
            return rule
                .nodes_mut()
                .expect("rule has block")
                .drain(..)
                .collect();
        }

        // JS: `let prefixer = this.prefixer().add[rule.first.prop]`. Our
        // sub-Prefixes' `add_table` is `IndexMap<String, Vec<String>>`
        // — the post-`select()` per-name prefix list. Until `preprocess()`
        // wires Prefixer instances on top of that, there's no `process`
        // method to dispatch through. Mirror JS's truthy-check pattern:
        // if there's no entry, skip — a no-op that's byte-equivalent to
        // JS for the same empty-preprocess state.
        let prop_name = match &first.kind {
            NodeKind::Declaration(d) => d.prop.clone(),
            _ => unreachable!("virtual_rule yields Declaration"),
        };
        let _prefixer_entry = self.prefixer(all).add_table.get(&prop_name).cloned();
        // TODO(agent-4): once `preprocess` lands, dispatch through a
        // Prefixer subclass instance keyed off `_prefixer_entry`. The
        // dispatch will mutate `rule.first` (insertBefore prefixed clones)
        // through the standard base-class API.

        // JS: the values loop calls `value.process(decl)` for each
        // value-prefixer registered against `rule.first.prop`. Currently
        // `Prefixes::values('add', prop)` returns an empty Vec stub
        // (AGENT_1's note), so the loop body never runs. Mirror that.
        let _values = self.prefixer(all).values("add", &prop_name);
        // for value in values { value.process(&mut decl) ... }

        // JS: `Value.save(this.all, decl)` flushes any cached
        // `_autoprefixerValues` map back onto the decl's `value`. With
        // no prefixer having populated that map, this is a no-op. Skip
        // until `Value::save` is wired through `processor.rs`.

        rule.nodes_mut()
            .expect("rule has block")
            .drain(..)
            .collect()
    }

    /// JS: `isNot(node) { return typeof node === 'string' && /not\s*/i.test(node) }`.
    ///
    /// JS bug-feature: `\s*` is zero-or-more, so `"not"` alone matches.
    /// Also — the regex has no anchors, so any text containing the
    /// substring `"not"` (case-insensitive) matches. Bug → preserved.
    pub fn is_not(node: &BracketNode) -> bool {
        static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)not\s*").unwrap());
        match node {
            BracketNode::Text(s) => RE.is_match(s),
            _ => false,
        }
    }

    /// JS: `isOr(node) { return typeof node === 'string' && /\s*or\s*/i.test(node) }`.
    pub fn is_or(node: &BracketNode) -> bool {
        static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\s*or\s*").unwrap());
        match node {
            BracketNode::Text(s) => RE.is_match(s),
            _ => false,
        }
    }

    /// JS:
    /// ```js
    /// isProp(node) {
    ///   return typeof node === 'object' && node.length === 1 && typeof node[0] === 'string'
    /// }
    /// ```
    ///
    /// Group with exactly one child, that child being a Text. Used to
    /// recognise `(prop: value)` clauses among the bracket tree's nodes.
    pub fn is_prop(node: &BracketNode) -> bool {
        match node {
            BracketNode::Group(g) => {
                g.len() == 1 && matches!(g[0], BracketNode::Text(_))
            }
            _ => false,
        }
    }

    /// JS:
    /// ```js
    /// isHack(all, unprefixed) {
    ///   let check = new RegExp(`(\\(|\\s)${utils.escapeRegexp(unprefixed)}:`)
    ///   return !check.test(all)
    /// }
    /// ```
    ///
    /// Returns true when the prefixed property has NO unprefixed
    /// equivalent in the same query string — i.e., it's a "hack" worth
    /// keeping. Compiles a fresh regex per call (mirroring JS).
    pub fn is_hack(all: &str, unprefixed: &str) -> bool {
        let pattern = format!(r"(\(|\s){}:", utils::escape_regexp(unprefixed));
        let re = Regex::new(&pattern).expect("valid is_hack regex");
        !re.is_match(all)
    }

    /// JS:
    /// ```js
    /// toRemove(str, all) {
    ///   let [prop, value] = this.parse(str)
    ///   let unprefixed = this.all.unprefixed(prop)
    ///   let cleaner = this.all.cleaner()
    ///   if (cleaner.remove[prop] && cleaner.remove[prop].remove && !this.isHack(all, unprefixed)) return true
    ///   for (let checker of cleaner.values('remove', unprefixed)) {
    ///     if (checker.check(value)) return true
    ///   }
    ///   return false
    /// }
    /// ```
    ///
    /// Both branches require AGENT_4's `preprocess()`-built shapes:
    ///   * `cleaner.remove[prop].remove` — a marker on the per-prop entry
    ///     of `cleaner.remove`. Today `Prefixes::remove_table` is a flat
    ///     `IndexMap<String, Vec<String>>` — no per-prop marker shape.
    ///   * `cleaner.values('remove', prop)` — currently returns an empty
    ///     `Vec<String>` stub.
    ///
    /// Both branches resolve to "no removal" in the empty-preprocess
    /// state, which is byte-equivalent to JS in the same state. So the
    /// JS-correct return today is `false`. The pure pieces (`parse`,
    /// `unprefixed_prop`, `is_hack`) are still exercised so a regression
    /// in any of them shows up before `preprocess` lands.
    pub fn to_remove(&self, s: &str, all_str: &str, all: &Prefixes) -> bool {
        let (prop, _value) = self.parse(s);
        let unprefixed = all.unprefixed_prop(&prop);
        let cleaner = all.cleaner();

        // JS: `cleaner.remove[prop] && cleaner.remove[prop].remove && !this.isHack(all, unprefixed)`.
        // The `.remove` marker only exists on @keyframes/@viewport-style
        // entries that `preprocess()` writes (`remove[prefixed] = { remove: true }`).
        // The flat `remove_table` we hold today is the pre-preprocess
        // shape — no marker — so this branch never fires.
        if cleaner.remove_table.contains_key(&prop) {
            // TODO(agent-4): once preprocess marks @keyframes/@viewport
            // entries with `.remove = true`, gate on that marker here.
            // The `is_hack` arm is already correctly factored.
            let _ = Self::is_hack(all_str, &unprefixed);
        }

        // JS: `for (let checker of cleaner.values('remove', unprefixed))`.
        // Empty until `preprocess()` populates value prefixers; loop
        // body never executes today.
        for _checker in cleaner.values("remove", &unprefixed) {
            // TODO(agent-4): `checker.check(value)` once value prefixers
            // expose a `check` method.
        }

        false
    }

    /// JS:
    /// ```js
    /// remove(nodes, all) {
    ///   let i = 0
    ///   while (i < nodes.length) {
    ///     if (!this.isNot(nodes[i - 1]) && this.isProp(nodes[i]) && this.isOr(nodes[i + 1])) {
    ///       if (this.toRemove(nodes[i][0], all)) {
    ///         nodes.splice(i, 2)
    ///         continue
    ///       }
    ///       i += 2
    ///       continue
    ///     }
    ///     if (typeof nodes[i] === 'object') {
    ///       nodes[i] = this.remove(nodes[i], all)
    ///     }
    ///     i += 1
    ///   }
    ///   return nodes
    /// }
    /// ```
    ///
    /// Walks the bracket tree dropping `(prop: value) or` pairs whose
    /// `toRemove` returns true. JS quirk: index `-1` and `len` are
    /// `undefined` → `isNot(undefined)` / `isOr(undefined)` are false,
    /// so the boundary checks fall through gracefully.
    pub fn remove(
        &self,
        mut nodes: Vec<BracketNode>,
        all_str: &str,
        all: &Prefixes,
    ) -> Vec<BracketNode> {
        let mut i: usize = 0;
        while i < nodes.len() {
            let prev_is_not = i
                .checked_sub(1)
                .and_then(|p| nodes.get(p))
                .map(Self::is_not)
                .unwrap_or(false);
            let cur_is_prop = Self::is_prop(&nodes[i]);
            let next_is_or = nodes
                .get(i + 1)
                .map(Self::is_or)
                .unwrap_or(false);

            if !prev_is_not && cur_is_prop && next_is_or {
                let prop_str = match &nodes[i] {
                    BracketNode::Group(g) => match &g[0] {
                        BracketNode::Text(t) => t.clone(),
                        _ => unreachable!("is_prop guarantees Text inside Group"),
                    },
                    _ => unreachable!("is_prop guarantees Group"),
                };
                if self.to_remove(&prop_str, all_str, all) {
                    // `nodes.splice(i, 2)` — drop the `(prop: value)` AND
                    // the following ` or `. Cursor stays at `i`.
                    nodes.drain(i..i + 2);
                    continue;
                }
                i += 2;
                continue;
            }

            if let BracketNode::Group(inner) = &nodes[i] {
                let inner_clone = inner.clone();
                nodes[i] = BracketNode::Group(self.remove(inner_clone, all_str, all));
            }
            i += 1;
        }
        nodes
    }

    /// JS:
    /// ```js
    /// cleanBrackets(nodes) {
    ///   return nodes.map(i => {
    ///     if (typeof i !== 'object') return i
    ///     if (i.length === 1 && typeof i[0] === 'object') return this.cleanBrackets(i[0])
    ///     return this.cleanBrackets(i)
    ///   })
    /// }
    /// ```
    ///
    /// Strips one level of redundant nesting `((x))` → `(x)`. The JS
    /// `return this.cleanBrackets(i[0])` returns an array, which the
    /// outer `.map(...)` packs back as an element — `stringify` then
    /// wraps that element in `()` because it's an array. Our equivalent
    /// is `Group(clean_brackets(inner))`.
    pub fn clean_brackets(&self, nodes: &[BracketNode]) -> Vec<BracketNode> {
        nodes
            .iter()
            .map(|i| match i {
                BracketNode::Text(_) => i.clone(),
                BracketNode::Group(g) => {
                    if g.len() == 1 {
                        if let BracketNode::Group(inner) = &g[0] {
                            return BracketNode::Group(self.clean_brackets(inner));
                        }
                    }
                    BracketNode::Group(self.clean_brackets(g))
                }
            })
            .collect()
    }

    /// JS:
    /// ```js
    /// convert(progress) {
    ///   let result = ['']
    ///   for (let i of progress) {
    ///     result.push([`${i.prop}: ${i.value}`])
    ///     result.push(' or ')
    ///   }
    ///   result[result.length - 1] = ''
    ///   return result
    /// }
    /// ```
    ///
    /// Builds a bracket-list of `(prop: value)` clauses joined by `' or '`.
    /// Empty input → `['']` (after the no-op last-slot assignment).
    pub fn convert(&self, progress: &[CoreNode]) -> Vec<BracketNode> {
        let mut result: Vec<BracketNode> = vec![BracketNode::Text(String::new())];
        for decl in progress {
            if let NodeKind::Declaration(d) = &decl.kind {
                result.push(BracketNode::Group(vec![BracketNode::Text(format!(
                    "{}: {}",
                    d.prop, d.value
                ))]));
                result.push(BracketNode::Text(" or ".to_string()));
            }
        }
        if let Some(last) = result.last_mut() {
            *last = BracketNode::Text(String::new());
        }
        result
    }

    /// JS:
    /// ```js
    /// normalize(nodes) {
    ///   if (typeof nodes !== 'object') return nodes
    ///   nodes = nodes.filter(i => i !== '')
    ///   if (typeof nodes[0] === 'string') {
    ///     let firstNode = nodes[0].trim()
    ///     if (firstNode.includes(':') || firstNode === 'selector' || firstNode === 'not selector') {
    ///       return [brackets.stringify(nodes)]
    ///     }
    ///   }
    ///   return nodes.map(i => this.normalize(i))
    /// }
    /// ```
    ///
    /// Folds nested function-call groups back into a single text token
    /// when the bracket-list represents a `prop: value(...)` clause.
    /// Recursion only descends into Group children; Text children pass
    /// through (matching JS `typeof i !== 'object'` early return).
    pub fn normalize(&self, nodes: &[BracketNode]) -> Vec<BracketNode> {
        // `nodes.filter(i => i !== '')` — drop only empty Text nodes.
        let filtered: Vec<BracketNode> = nodes
            .iter()
            .filter(|n| !matches!(n, BracketNode::Text(t) if t.is_empty()))
            .cloned()
            .collect();

        if let Some(BracketNode::Text(first)) = filtered.first() {
            let first_trim = first.trim();
            if first_trim.contains(':')
                || first_trim == "selector"
                || first_trim == "not selector"
            {
                return vec![BracketNode::Text(brackets::stringify(&filtered))];
            }
        }

        filtered
            .into_iter()
            .map(|n| match n {
                BracketNode::Text(_) => n,
                BracketNode::Group(inner) => BracketNode::Group(self.normalize(&inner)),
            })
            .collect()
    }

    /// JS:
    /// ```js
    /// add(nodes, all) {
    ///   return nodes.map(i => {
    ///     if (this.isProp(i)) {
    ///       let prefixed = this.prefixed(i[0])
    ///       if (prefixed.length > 1) return this.convert(prefixed)
    ///       return i
    ///     }
    ///     if (typeof i === 'object') return this.add(i, all)
    ///     return i
    ///   })
    /// }
    /// ```
    ///
    /// Recursively walks the bracket tree. For each `(prop: value)`
    /// clause it queries the prefixer for prefixed equivalents; if more
    /// than the input clause comes back, replaces with the `(... or ...)`
    /// expansion. Otherwise leaves the clause alone.
    ///
    /// Renamed `add_prefixes` to avoid clashing with `Vec::push`-style
    /// `add` reads — semantics match JS `add` exactly.
    pub fn add_prefixes(
        &mut self,
        nodes: &[BracketNode],
        all_str: &str,
        all: &Prefixes,
    ) -> Vec<BracketNode> {
        // Avoid double-borrow of `self` inside an iterator closure.
        let mut out: Vec<BracketNode> = Vec::with_capacity(nodes.len());
        for i in nodes {
            if Self::is_prop(i) {
                let s = match i {
                    BracketNode::Group(g) => match &g[0] {
                        BracketNode::Text(t) => t.clone(),
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                };
                let prefixed = self.prefixed(&s, all);
                if prefixed.len() > 1 {
                    out.push(BracketNode::Group(self.convert(&prefixed)));
                } else {
                    out.push(i.clone());
                }
                continue;
            }
            if let BracketNode::Group(inner) = i {
                out.push(BracketNode::Group(self.add_prefixes(inner, all_str, all)));
            } else {
                out.push(i.clone());
            }
        }
        out
    }

    /// JS:
    /// ```js
    /// process(rule) {
    ///   let ast = brackets.parse(rule.params)
    ///   ast = this.normalize(ast)
    ///   ast = this.remove(ast, rule.params)
    ///   ast = this.add(ast, rule.params)
    ///   ast = this.cleanBrackets(ast)
    ///   rule.params = brackets.stringify(ast)
    /// }
    /// ```
    ///
    /// Entry point. Mutates `rule.params` in place. `processor.rs`
    /// (AGENT_4) calls this for every `@supports` at-rule it encounters.
    pub fn process(&mut self, rule: &mut CoreNode, all: &Prefixes) {
        let params = match &rule.kind {
            NodeKind::AtRule(at) => at.params.clone(),
            _ => return,
        };
        let ast = brackets::parse(&params);
        let ast = self.normalize(&ast);
        let ast = self.remove(ast, &params, all);
        let ast = self.add_prefixes(&ast, &params, all);
        let ast = self.clean_brackets(&ast);
        let new_params = brackets::stringify(&ast);
        if let NodeKind::AtRule(at) = &mut rule.kind {
            at.params = new_params;
        }
    }

    /// JS:
    /// ```js
    /// disabled(node) {
    ///   if (!this.all.options.grid) {
    ///     if (node.prop === 'display' && node.value.includes('grid')) return true
    ///     if (node.prop.includes('grid') || node.prop === 'justify-items') return true
    ///   }
    ///   if (this.all.options.flexbox === false) {
    ///     if (node.prop === 'display' && node.value.includes('flex')) return true
    ///     let other = ['order', 'justify-content', 'align-items', 'align-content']
    ///     if (node.prop.includes('flex') || other.includes(node.prop)) return true
    ///   }
    ///   return false
    /// }
    /// ```
    ///
    /// `grid_enabled` mirrors JS truthiness on `options.grid` — `true`
    /// when grid handling is opted-in, `false` (the default) when off.
    /// `flexbox_disabled` mirrors `options.flexbox === false` (strict
    /// equality) — `true` only when flexbox is *explicitly* disabled.
    pub fn disabled_with(
        node: &CoreNode,
        grid_enabled: bool,
        flexbox_disabled: bool,
    ) -> bool {
        let (prop, value) = match &node.kind {
            NodeKind::Declaration(d) => (d.prop.as_str(), d.value.as_str()),
            _ => return false,
        };

        if !grid_enabled {
            if prop == "display" && value.contains("grid") {
                return true;
            }
            if prop.contains("grid") || prop == "justify-items" {
                return true;
            }
        }

        if flexbox_disabled {
            if prop == "display" && value.contains("flex") {
                return true;
            }
            let other = ["order", "justify-content", "align-items", "align-content"];
            if prop.contains("flex") || other.iter().any(|o| *o == prop) {
                return true;
            }
        }

        false
    }

    /// JS `disabled(node)` — exact shape, with options pulled off `all`.
    ///
    /// `all.options.grid` is `Option<String>` in `PrefixesOptions`;
    /// `Some(_)` is JS-truthy, `None` is JS-falsy → grid_enabled =
    /// `is_some()`.
    ///
    /// `all.options.flexbox` is also `Option<String>`. JS allows
    /// `false | 'no-2009'` (or unset/string) — strict `=== false` is
    /// only true for boolean `false`. The current Rust shape can't
    /// distinguish boolean `false` from "unset", so the flexbox branch
    /// never fires here. Documented in `AGENT_2_DONE.md`; AGENT_1 will
    /// extend `PrefixesOptions::flexbox` (e.g. to a small enum) when
    /// any consumer needs the explicit-disable case.
    pub fn disabled(&self, node: &CoreNode, all: &Prefixes) -> bool {
        let grid_enabled = all.options.grid.is_some();
        let flexbox_disabled = false;
        Self::disabled_with(node, grid_enabled, flexbox_disabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsers::{BrowserslistOpts, BrowsersOptions};
    use crate::prefixes::PrefixesOptions;
    use std::path::PathBuf;

    fn t(s: &str) -> BracketNode {
        BracketNode::Text(s.to_string())
    }
    fn g(items: Vec<BracketNode>) -> BracketNode {
        BracketNode::Group(items)
    }

    fn afm_fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("browserslist-shim")
            .join("tests")
            .join("fixtures")
            .join("afm")
    }

    fn afm_browsers() -> Browsers {
        let opts = BrowsersOptions {
            from: Some(afm_fixture_dir().to_string_lossy().into_owned()),
        };
        Browsers::new(Vec::new(), opts, BrowserslistOpts::default())
    }

    fn empty_browsers() -> Browsers {
        Browsers {
            selected: Vec::new(),
            options: BrowsersOptions::default(),
            browserslist_opts: BrowserslistOpts::default(),
        }
    }

    fn dummy_prefixes() -> Prefixes {
        Prefixes::new(empty_browsers(), PrefixesOptions::default())
    }

    fn afm_prefixes() -> Prefixes {
        Prefixes::new(afm_browsers(), PrefixesOptions::default())
    }

    // ----- predicates ---------------------------------------------------

    #[test]
    fn is_not_matches_string_with_not() {
        assert!(Supports::is_not(&t("not")));
        assert!(Supports::is_not(&t("NOT ")));
        assert!(Supports::is_not(&t(" not ")));
        // bug-for-bug: any text containing "not" matches (no anchor).
        assert!(Supports::is_not(&t("cannotyz")));
        assert!(!Supports::is_not(&t("foo")));
        assert!(!Supports::is_not(&g(vec![t("not")])));
    }

    #[test]
    fn is_or_matches_string_with_or() {
        assert!(Supports::is_or(&t(" or ")));
        assert!(Supports::is_or(&t("OR")));
        assert!(!Supports::is_or(&t("and")));
        assert!(!Supports::is_or(&g(vec![t("or")])));
    }

    #[test]
    fn is_prop_only_matches_single_text_group() {
        assert!(Supports::is_prop(&g(vec![t("display: flex")])));
        assert!(!Supports::is_prop(&g(vec![])));
        assert!(!Supports::is_prop(&g(vec![t("a"), t("b")])));
        assert!(!Supports::is_prop(&g(vec![g(vec![t("nested")])])));
        assert!(!Supports::is_prop(&t("display: flex")));
    }

    // ----- parse --------------------------------------------------------

    #[test]
    fn parse_splits_on_first_colon_only() {
        let s = Supports::new();
        assert_eq!(s.parse("display: flex"), ("display".into(), "flex".into()));
        // Third colon is dropped — JS `parts[1]` is just the second split.
        assert_eq!(s.parse("a:b:c"), ("a".into(), "b".into()));
    }

    #[test]
    fn parse_handles_missing_value() {
        let s = Supports::new();
        assert_eq!(s.parse("display"), ("display".into(), "".into()));
        assert_eq!(s.parse(""), ("".into(), "".into()));
    }

    #[test]
    fn parse_trims_both_sides() {
        let s = Supports::new();
        assert_eq!(
            s.parse("  display  :   flex  "),
            ("display".into(), "flex".into())
        );
    }

    // ----- is_hack ------------------------------------------------------

    #[test]
    fn is_hack_true_when_no_unprefixed_equivalent() {
        assert!(Supports::is_hack("(-webkit-foo: bar)", "display"));
    }

    #[test]
    fn is_hack_false_when_unprefixed_present_in_query() {
        assert!(!Supports::is_hack(
            "(-webkit-display: flex) or (display: flex)",
            "display"
        ));
    }

    #[test]
    fn is_hack_anchors_to_open_paren_or_whitespace() {
        // `mydisplay:` should NOT defeat the hack check — must follow `(` or `\s`.
        assert!(Supports::is_hack("(mydisplay: foo)", "display"));
    }

    // ----- clean_brackets -----------------------------------------------

    #[test]
    fn clean_brackets_strips_redundant_outer_group() {
        let s = Supports::new();
        // `((a:b))` → `(a:b)`
        let input = vec![g(vec![g(vec![t("a:b")])])];
        let out = s.clean_brackets(&input);
        assert_eq!(out, vec![g(vec![t("a:b")])]);
    }

    #[test]
    fn clean_brackets_leaves_text_alone() {
        let s = Supports::new();
        let input = vec![t("hello"), g(vec![t("a:b")]), t(" or ")];
        let out = s.clean_brackets(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn clean_brackets_recurses_into_multi_child_groups() {
        let s = Supports::new();
        let input = vec![g(vec![t("x"), t(" or "), g(vec![g(vec![t("y")])])])];
        let out = s.clean_brackets(&input);
        // Outer Group: multi-child → recurse:
        //   t("x") passes
        //   t(" or ") passes
        //   g([g([t("y")])]) — single inner Group → unwrap to g([t("y")]).
        assert_eq!(out, vec![g(vec![t("x"), t(" or "), g(vec![t("y")])])]);
    }

    // ----- normalize ----------------------------------------------------

    #[test]
    fn normalize_collapses_simple_property() {
        let s = Supports::new();
        let parsed = brackets::parse("display: flex");
        let out = s.normalize(&parsed);
        assert_eq!(out, vec![t("display: flex")]);
    }

    #[test]
    fn normalize_collapses_nested_function() {
        let s = Supports::new();
        // brackets.parse("transform: rotate(180deg)") yields:
        //   [Text("transform: rotate"), Group([Text("180deg")]), Text("")]
        // First elem is Text "transform: rotate" → contains ':' → collapse.
        let parsed = brackets::parse("transform: rotate(180deg)");
        let out = s.normalize(&parsed);
        assert_eq!(out, vec![t("transform: rotate(180deg)")]);
    }

    #[test]
    fn normalize_recurses_into_group_when_first_is_group() {
        let s = Supports::new();
        // `(display: flex)` →
        //   [Text(""), Group([Text("display: flex")]), Text("")]
        // After filter: [Group(...)]. First elem is Group, not Text →
        // map recurses.
        let parsed = brackets::parse("(display: flex)");
        let out = s.normalize(&parsed);
        assert_eq!(out, vec![g(vec![t("display: flex")])]);
    }

    #[test]
    fn normalize_filters_empty_text() {
        let s = Supports::new();
        let input = vec![t(""), g(vec![t("display: flex")]), t("")];
        let out = s.normalize(&input);
        assert_eq!(out, vec![g(vec![t("display: flex")])]);
    }

    #[test]
    fn normalize_recognises_selector_token() {
        let s = Supports::new();
        let input = vec![t("selector"), g(vec![t(":focus-visible")])];
        let out = s.normalize(&input);
        assert_eq!(out, vec![t("selector(:focus-visible)")]);
    }

    // ----- convert ------------------------------------------------------

    #[test]
    fn convert_empty_progress() {
        let s = Supports::new();
        let out = s.convert(&[]);
        assert_eq!(out, vec![t("")]);
    }

    #[test]
    fn convert_single_decl() {
        let s = Supports::new();
        let mut decl = CoreNode::new(NodeKind::Declaration(
            postcss_core::declaration::Declaration {
                prop: "display".into(),
                value: "flex".into(),
                important: false,
                variable: false,
            },
        ));
        decl.raws.before = Some(String::new());
        let out = s.convert(&[decl]);
        assert_eq!(out, vec![t(""), g(vec![t("display: flex")]), t("")]);
    }

    #[test]
    fn convert_two_decls_joined_by_or() {
        let s = Supports::new();
        let make = |prop: &str, value: &str| -> CoreNode {
            CoreNode::new(NodeKind::Declaration(
                postcss_core::declaration::Declaration {
                    prop: prop.into(),
                    value: value.into(),
                    important: false,
                    variable: false,
                },
            ))
        };
        let decls = vec![make("display", "flex"), make("display", "-webkit-flex")];
        let out = s.convert(&decls);
        assert_eq!(brackets::stringify(&out), "(display: flex) or (display: -webkit-flex)");
    }

    // ----- supported list -----------------------------------------------

    #[test]
    fn supported_list_is_non_empty() {
        let s: &Vec<String> = &SUPPORTED;
        assert!(!s.is_empty(), "SUPPORTED should not be empty");
        // chrome 28 was the first stable to ship @supports.
        assert!(s.iter().any(|b| b == "chrome 28"));
    }

    // ----- virtual_rule -------------------------------------------------

    #[test]
    fn virtual_rule_yields_rule_with_one_decl() {
        let s = Supports::new();
        let rule = s.virtual_rule("display: flex");
        let kids = rule.nodes().expect("rule has a block");
        assert_eq!(kids.len(), 1);
        match &kids[0].kind {
            NodeKind::Declaration(d) => {
                assert_eq!(d.prop, "display");
                assert_eq!(d.value, "flex");
            }
            other => panic!("expected Declaration, got {:?}", other),
        }
        // `raws.before = ''` is load-bearing — Value.save reads it.
        assert_eq!(kids[0].raws.before.as_deref(), Some(""));
    }

    // ----- prefixed (empty-preprocess state, returns single decl) -------

    #[test]
    fn prefixed_returns_bare_decl_when_no_prefixers_registered() {
        // Until `preprocess()` lands (AGENT_4), no Prefixer is wired so
        // the inner `prefixer.process` and value-loop calls are no-ops.
        // `prefixed` should return the single virtual decl unchanged.
        let mut s = Supports::new();
        let all = afm_prefixes();
        let out = s.prefixed("display: flex", &all);
        assert_eq!(out.len(), 1);
        match &out[0].kind {
            NodeKind::Declaration(d) => {
                assert_eq!(d.prop, "display");
                assert_eq!(d.value, "flex");
            }
            _ => panic!("expected Declaration"),
        }
    }

    // ----- to_remove (empty-preprocess state, returns false) ------------

    #[test]
    fn to_remove_returns_false_in_empty_preprocess_state() {
        let s = Supports::new();
        let all = afm_prefixes();
        // No remove prefixer registered → JS-equivalent behaviour is
        // false. (`unprefixed_prop` and `is_hack` still get exercised
        // through the body; a regression in either fails compilation.)
        assert!(!s.to_remove("-webkit-display: flex", "(-webkit-display: flex)", &all));
    }

    // ----- end-to-end pipeline (passes today: no-op for unsupported) ---

    #[test]
    fn process_leaves_unsupported_query_unchanged() {
        let mut s = Supports::new();
        let all = afm_prefixes();
        let mut at_rule = CoreNode::new(NodeKind::AtRule(
            postcss_core::at_rule::AtRule {
                name: "supports".into(),
                params: "(display: flex)".into(),
                has_block: true,
                nodes: Vec::new(),
            },
        ));
        s.process(&mut at_rule, &all);
        // No prefixers registered → no expansion → params unchanged.
        match &at_rule.kind {
            NodeKind::AtRule(at) => assert_eq!(at.params, "(display: flex)"),
            _ => panic!("expected AtRule"),
        }
    }

    #[test]
    fn process_leaves_nested_function_query_unchanged() {
        let mut s = Supports::new();
        let all = afm_prefixes();
        let mut at_rule = CoreNode::new(NodeKind::AtRule(
            postcss_core::at_rule::AtRule {
                name: "supports".into(),
                params: "(transform: rotate(45deg))".into(),
                has_block: true,
                nodes: Vec::new(),
            },
        ));
        s.process(&mut at_rule, &all);
        match &at_rule.kind {
            NodeKind::AtRule(at) => assert_eq!(at.params, "(transform: rotate(45deg))"),
            _ => panic!("expected AtRule"),
        }
    }

    #[test]
    fn process_normalises_double_parens() {
        // The pipeline runs cleanBrackets last, so `((display: flex))`
        // → `(display: flex)` even with no prefixers registered.
        let mut s = Supports::new();
        let all = afm_prefixes();
        let mut at_rule = CoreNode::new(NodeKind::AtRule(
            postcss_core::at_rule::AtRule {
                name: "supports".into(),
                params: "((display: flex))".into(),
                has_block: true,
                nodes: Vec::new(),
            },
        ));
        s.process(&mut at_rule, &all);
        match &at_rule.kind {
            NodeKind::AtRule(at) => assert_eq!(at.params, "(display: flex)"),
            _ => panic!("expected AtRule"),
        }
    }

    // ----- prefixer (sub-Prefixes constructor wired through Prefixes::new) -

    #[test]
    fn prefixer_filters_to_supports_supporting_browsers() {
        // The cache is populated on first call; the filtered Browsers'
        // `selected` should be a subset of `all.browsers.selected` AND
        // every entry should appear in `SUPPORTED`.
        let all = afm_prefixes();
        let original_selected = all.browsers.selected.clone();
        let mut s = Supports::new();
        let p = s.prefixer(&all);
        for b in &p.browsers.selected {
            assert!(
                SUPPORTED.iter().any(|x| x == b),
                "filtered browser {b} not in SUPPORTED list"
            );
        }
        // Filtered list is a subset of the input.
        for b in &p.browsers.selected {
            assert!(original_selected.iter().any(|o| o == b));
        }
    }

    #[test]
    fn prefixer_caches_across_calls() {
        let all = afm_prefixes();
        let mut s = Supports::new();
        let first_ptr = s.prefixer(&all) as *const Prefixes;
        let second_ptr = s.prefixer(&all) as *const Prefixes;
        assert!(std::ptr::eq(first_ptr, second_ptr));
    }

    // ----- disabled (with explicit options) -----------------------------

    fn make_decl(prop: &str, value: &str) -> CoreNode {
        CoreNode::new(NodeKind::Declaration(
            postcss_core::declaration::Declaration {
                prop: prop.into(),
                value: value.into(),
                important: false,
                variable: false,
            },
        ))
    }

    #[test]
    fn disabled_grid_off_blocks_grid_decls() {
        assert!(Supports::disabled_with(&make_decl("display", "grid"), false, false));
        assert!(Supports::disabled_with(&make_decl("grid-area", "x"), false, false));
        assert!(Supports::disabled_with(&make_decl("justify-items", "y"), false, false));
        assert!(!Supports::disabled_with(&make_decl("color", "red"), false, false));
    }

    #[test]
    fn disabled_grid_on_lets_grid_through() {
        assert!(!Supports::disabled_with(&make_decl("display", "grid"), true, false));
        assert!(!Supports::disabled_with(&make_decl("grid-area", "x"), true, false));
    }

    #[test]
    fn disabled_flexbox_explicitly_off_blocks_flex_decls() {
        assert!(Supports::disabled_with(&make_decl("display", "flex"), true, true));
        assert!(Supports::disabled_with(&make_decl("flex-grow", "1"), true, true));
        assert!(Supports::disabled_with(&make_decl("order", "1"), true, true));
        assert!(Supports::disabled_with(&make_decl("justify-content", "center"), true, true));
        assert!(Supports::disabled_with(&make_decl("align-items", "center"), true, true));
        assert!(Supports::disabled_with(&make_decl("align-content", "stretch"), true, true));
    }

    #[test]
    fn disabled_flexbox_default_lets_flex_through() {
        // `flexbox_disabled = false` (JS `options.flexbox !== false`).
        assert!(!Supports::disabled_with(&make_decl("display", "flex"), true, false));
        assert!(!Supports::disabled_with(&make_decl("flex-grow", "1"), true, false));
    }

    #[test]
    fn disabled_pulls_grid_from_prefixes_options() {
        let s = Supports::new();
        // Default options: grid is None → grid_enabled = false → disabled.
        let all_default = dummy_prefixes();
        assert!(s.disabled(&make_decl("display", "grid"), &all_default));
        // With grid set → grid_enabled = true → not disabled.
        let all_grid = Prefixes::new(
            empty_browsers(),
            PrefixesOptions { grid: Some("autoplace".into()), ..Default::default() },
        );
        assert!(!s.disabled(&make_decl("display", "grid"), &all_grid));
    }

    // ----- remove on text-only nodes is a no-op -------------------------

    #[test]
    fn remove_does_not_recurse_into_text_only_tree() {
        let s = Supports::new();
        let all = dummy_prefixes();
        let input = vec![t("not "), t("anything")];
        let out = s.remove(input.clone(), "", &all);
        assert_eq!(out, input);
    }
}
