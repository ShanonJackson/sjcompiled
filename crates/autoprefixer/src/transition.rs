//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/transition.js`.
//!
//! `class Transition` — handles the `transition` shorthand declaration.
//! When a `transition` value contains a property name (e.g. `transform 0.3s`),
//! the transition needs prefix-matched siblings on the declaration.
//!
//! ## Translation notes
//!
//! - JS `let { list } = require('postcss')` → `postcss_core::list`.
//! - JS `let parser = require('postcss-value-parser')` → `postcss_value_parser`.
//! - JS `decl.cloneBefore({ prop, value })` is `clone_node(decl)` + mutate
//!   prop/value + `insert_before_at_path`. `clone_node` strips
//!   `_autoprefixer*` attrs per `prefixer::CLONE_STRIP_KEYS`.
//! - JS `decl.warn(result, msg)` collects into the supplied
//!   `&mut Vec<String>`. Diagnostic only — not on the byte-output hashing
//!   path — but kept for parity.
//! - `this.prefixes.{add,remove,prefixed,unprefixed,options}` are abstracted
//!   into the [`TransitionPrefixesView`] trait so we can implement and
//!   test without depending on `Prefixes::new` (AGENT_1's territory).
//!
//! ## Cursor-shift bug
//!
//! Each `cloneBefore` call is `insert_before_at_path` which shifts the
//! original decl's index up by one. The cursor-bump pattern from
//! `at_rule.rs::process` is the fix — bump the path's last index after
//! every successful insert. See HANDOVER.md §3.

use postcss_core::{
    insert_before_at_path, list, node_at_path, node_at_path_mut, parent_some, Node, NodeKind,
};
use postcss_value_parser::{
    parse as vp_parse, stringify as vp_stringify, Node as VNode, NodeKind as VNodeKind,
};

use crate::browsers::Browsers;
use crate::prefixer::clone_node;
use crate::vendor;

/// JS `prefixes.options.flexbox`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexboxOption {
    /// JS default — flexbox prefixes enabled.
    On,
    /// JS `flexbox: false` — disabled.
    Off,
    /// JS `flexbox: 'no-2009'` — exclude 2009-spec prefixes.
    No2009,
}

impl Default for FlexboxOption {
    fn default() -> Self {
        FlexboxOption::On
    }
}

/// View into the `Prefixes` orchestrator that `Transition` consumes.
/// `Prefixes` (AGENT_1) will impl this; tests use a local mock.
pub trait TransitionPrefixesView {
    /// JS: `this.prefixes.add[prop]` then `.prefixes` if it exists.
    /// Returns `None` when `add[prop]` is undefined OR when the entry has
    /// no `.prefixes` field (e.g., the `selectors` slot).
    fn add_prefixes(&self, prop: &str) -> Option<&[String]>;

    /// JS: `this.prefixes.remove[prop]` then `.remove === true`.
    fn should_remove(&self, prop: &str) -> bool;

    /// JS: `this.prefixes.prefixed(prop, prefix)`. JS dispatches via
    /// `this.decl(prop).prefixed(prop, prefix)` which is `prefix + prop`
    /// for the default `Declaration` class (hacks may override).
    fn prefixed(&self, prop: &str, prefix: &str) -> String;

    /// JS: `this.prefixes.unprefixed(prop)`. Strips vendor prefix and
    /// applies decl-class normalization (`flex-direction → flex-flow`
    /// in the default impl).
    fn unprefixed(&self, prop: &str) -> String;

    /// JS: `this.prefixes.options.flexbox`.
    fn flexbox(&self) -> FlexboxOption {
        FlexboxOption::On
    }
}

/// JS: `this.props = ['transition', 'transition-property']`.
pub const TRANSITION_PROPS: &[&str] = &["transition", "transition-property"];

pub struct Transition<'a> {
    pub props: &'static [&'static str],
    pub prefixes: &'a dyn TransitionPrefixesView,
}

impl<'a> Transition<'a> {
    /// JS: `constructor(prefixes)`.
    pub fn new(prefixes: &'a dyn TransitionPrefixesView) -> Self {
        Self {
            props: TRANSITION_PROPS,
            prefixes,
        }
    }

    /// JS: `add(decl, result)`.
    /// Process transition and add prefixes for all necessary properties.
    /// Returns the number of warnings emitted (collected into `warnings`).
    pub fn add(&self, root: &mut Node, path: &[usize], warnings: &mut Vec<String>) {
        // Snapshot decl prop + value under an immutable borrow first.
        let (decl_prop, decl_value) = match node_at_path(root, path) {
            Some(n) => match &n.kind {
                NodeKind::Declaration(d) => (d.prop.clone(), d.value.clone()),
                _ => return,
            },
            None => return,
        };

        // JS: `let add = this.prefixes.add[decl.prop]`
        // JS: `let vendorPrefixes = this.ruleVendorPrefixes(decl)`
        // JS: `let declPrefixes = vendorPrefixes || (add && add.prefixes) || []`
        let vendor_prefixes = self.rule_vendor_prefixes(root, path);
        let decl_prefixes: Vec<String> = match &vendor_prefixes {
            Some(vps) => vps.clone(),
            None => self
                .prefixes
                .add_prefixes(&decl_prop)
                .map(<[String]>::to_vec)
                .unwrap_or_default(),
        };

        // JS: `let params = this.parse(decl.value)`
        let mut params = self.parse_value(&decl_value);
        // JS: `let names = params.map(i => this.findProp(i))`
        let names: Vec<String> = params.iter().map(|p| Self::find_prop(p)).collect();
        let mut added: Vec<Vec<VNode>> = Vec::new();

        // JS: `if (names.some(i => i[0] === '-')) return`
        if names.iter().any(|n| n.starts_with('-')) {
            return;
        }

        // JS: `for (let param of params) { ... }`
        for param in &params {
            let prop = Self::find_prop(param);
            if prop.starts_with('-') {
                continue;
            }

            // JS: `let prefixer = this.prefixes.add[prop]`
            // JS: `if (!prefixer || !prefixer.prefixes) continue`
            let prefixer_prefixes: Vec<String> = match self.prefixes.add_prefixes(&prop) {
                Some(p) => p.to_vec(),
                None => continue,
            };
            if prefixer_prefixes.is_empty() {
                continue;
            }

            for prefix in &prefixer_prefixes {
                // JS: `if (vendorPrefixes && !vendorPrefixes.some(p => prefix.includes(p))) continue`
                if let Some(vps) = &vendor_prefixes {
                    if !vps.iter().any(|p| prefix.contains(p.as_str())) {
                        continue;
                    }
                }

                // JS: `let prefixed = this.prefixes.prefixed(prop, prefix)`
                let prefixed = self.prefixes.prefixed(&prop, prefix);
                // JS: `if (prefixed !== '-ms-transform' && !names.includes(prefixed))`
                if prefixed != "-ms-transform" && !names.iter().any(|n| n == &prefixed) {
                    // JS: `if (!this.disabled(prop, prefix))`
                    if !self.disabled(&prop, prefix) {
                        // JS: `added.push(this.clone(prop, prefixed, param))`
                        added.push(Self::clone_param(&prop, &prefixed, param));
                    }
                }
            }
        }

        // JS: `params = params.concat(added)`
        params.extend(added);
        // JS: `let value = this.stringify(params)`
        let value = self.stringify_params(&mut params);

        // JS: `let webkitClean = this.stringify(this.cleanFromUnprefixed(params, '-webkit-'))`
        let webkit_clean = {
            let mut filtered = self.clean_from_unprefixed(&params, "-webkit-");
            self.stringify_params(&mut filtered)
        };

        // Track the path of the *original* decl through successive inserts.
        // Every `insert_before_at_path` shifts the original up by one.
        let mut current_path = path.to_vec();

        // JS: `if (declPrefixes.includes('-webkit-')) this.cloneBefore(decl, '-webkit-' + decl.prop, webkitClean)`
        if decl_prefixes.iter().any(|p| p == "-webkit-") {
            let prefixed_prop = format!("-webkit-{decl_prop}");
            if self.clone_before(root, &current_path, &prefixed_prop, &webkit_clean) {
                if let Some(last) = current_path.last_mut() {
                    *last += 1;
                }
            }
        }

        // JS: `this.cloneBefore(decl, decl.prop, webkitClean)`
        if self.clone_before(root, &current_path, &decl_prop, &webkit_clean) {
            if let Some(last) = current_path.last_mut() {
                *last += 1;
            }
        }

        // JS: `if (declPrefixes.includes('-o-')) { let operaClean = ...; this.cloneBefore(decl, '-o-' + decl.prop, operaClean) }`
        if decl_prefixes.iter().any(|p| p == "-o-") {
            let opera_clean = {
                let mut filtered = self.clean_from_unprefixed(&params, "-o-");
                self.stringify_params(&mut filtered)
            };
            let prefixed_prop = format!("-o-{decl_prop}");
            if self.clone_before(root, &current_path, &prefixed_prop, &opera_clean) {
                if let Some(last) = current_path.last_mut() {
                    *last += 1;
                }
            }
        }

        // JS: `for (prefix of declPrefixes) { if (prefix !== '-webkit-' && prefix !== '-o-') ... }`
        for prefix in &decl_prefixes {
            if prefix != "-webkit-" && prefix != "-o-" {
                let prefix_value = {
                    let filtered = self.clean_other_prefixes(&params, prefix);
                    let mut owned = filtered;
                    self.stringify_params(&mut owned)
                };
                let prefixed_prop = format!("{prefix}{decl_prop}");
                if self.clone_before(root, &current_path, &prefixed_prop, &prefix_value) {
                    if let Some(last) = current_path.last_mut() {
                        *last += 1;
                    }
                }
            }
        }

        // JS: `if (value !== decl.value && !this.already(decl, decl.prop, value)) { ... }`
        if value != decl_value
            && !self.already_at(root, &current_path, &decl_prop, &value)
        {
            // JS: `this.checkForWarning(result, decl)`
            self.check_for_warning(root, &current_path, warnings);
            // JS: `decl.cloneBefore()` — clone with same prop/value, insert before.
            if let Some(original) = node_at_path(root, &current_path) {
                let cloned = clone_node(original);
                insert_before_at_path(root, &current_path, cloned);
                if let Some(last) = current_path.last_mut() {
                    *last += 1;
                }
            }
            // JS: `decl.value = value`
            if let Some(here) = node_at_path_mut(root, &current_path) {
                if let NodeKind::Declaration(d) = &mut here.kind {
                    d.value = value;
                }
            }
        }
    }

    /// JS: `findProp(param)`.
    /// ```js
    /// let prop = param[0].value
    /// if (/^\d/.test(prop)) {
    ///   for (let [i, token] of param.entries()) {
    ///     if (i !== 0 && token.type === 'word') return token.value
    ///   }
    /// }
    /// return prop
    /// ```
    pub fn find_prop(param: &[VNode]) -> String {
        let prop = param.first().map(|n| n.value.clone()).unwrap_or_default();
        if prop.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            for (i, token) in param.iter().enumerate() {
                if i != 0 && token.kind == VNodeKind::Word {
                    return token.value.clone();
                }
            }
        }
        prop
    }

    /// JS: `already(decl, prop, value)`.
    /// `decl.parent.some(i => i.prop === prop && i.value === value)`.
    fn already_at(&self, root: &Node, path: &[usize], prop: &str, value: &str) -> bool {
        parent_some(root, path, |sibling| match &sibling.kind {
            NodeKind::Declaration(d) => d.prop == prop && d.value == value,
            _ => false,
        })
    }

    /// JS: `cloneBefore(decl, prop, value)`.
    /// ```js
    /// if (!this.already(decl, prop, value)) {
    ///   decl.cloneBefore({ prop, value })
    /// }
    /// ```
    /// Returns `true` if a clone was inserted (caller bumps the path).
    fn clone_before(&self, root: &mut Node, path: &[usize], prop: &str, value: &str) -> bool {
        if self.already_at(root, path, prop, value) {
            return false;
        }
        let original = match node_at_path(root, path) {
            Some(n) => n,
            None => return false,
        };
        let mut cloned = clone_node(original);
        if let NodeKind::Declaration(d) = &mut cloned.kind {
            d.prop = prop.to_string();
            d.value = value.to_string();
        }
        insert_before_at_path(root, path, cloned);
        true
    }

    /// JS: `checkForWarning(result, decl)`.
    /// Walks the decl's parent looking for a `transition-property` decl
    /// whose values include any prefixed prop AND another `transition-*`
    /// prop with multiple comma-separated values. Emits a warning when
    /// both conditions hold.
    fn check_for_warning(&self, root: &Node, path: &[usize], warnings: &mut Vec<String>) {
        let decl_prop = match node_at_path(root, path) {
            Some(n) => match &n.kind {
                NodeKind::Declaration(d) => d.prop.clone(),
                _ => return,
            },
            None => return,
        };
        if decl_prop != "transition-property" {
            return;
        }

        let parent = match path.split_last() {
            Some((_, parent_path)) => match node_at_path(root, parent_path) {
                Some(p) => p,
                None => return,
            },
            None => return,
        };
        let siblings = match parent.nodes() {
            Some(s) => s,
            None => return,
        };

        let mut is_prefixed = false;
        let mut has_associated_prop = false;

        for sibling in siblings {
            // JS: `if (i.type !== 'decl') return undefined` (skip)
            let (sib_prop, sib_value) = match &sibling.kind {
                NodeKind::Declaration(d) => (&d.prop, &d.value),
                _ => continue,
            };
            // JS: `if (i.prop.indexOf('transition-') !== 0) return undefined` (skip)
            if !sib_prop.starts_with("transition-") {
                continue;
            }
            let values = list::comma(sib_value);
            // JS: `if (i.prop === 'transition-property') { ... return undefined }`
            if sib_prop == "transition-property" {
                for v in &values {
                    if let Some(prefixes) = self.prefixes.add_prefixes(v) {
                        if !prefixes.is_empty() {
                            is_prefixed = true;
                        }
                    }
                }
                continue;
            }
            // JS: `hasAssociatedProp = hasAssociatedProp || values.length > 1`
            // JS: `return false` — `each` callback returning `false` stops
            //      iteration. Mirror by breaking out.
            has_associated_prop = has_associated_prop || values.len() > 1;
            break;
        }

        if is_prefixed && has_associated_prop {
            warnings.push(
                "Replace transition-property to transition, \
                 because Autoprefixer could not support \
                 any cases of transition-property \
                 and other transition-*"
                    .to_string(),
            );
        }
    }

    /// JS: `remove(decl)` — process transition and remove unnecessary
    /// properties.
    pub fn remove(&self, root: &mut Node, path: &[usize]) {
        let (decl_prop, decl_value) = match node_at_path(root, path) {
            Some(n) => match &n.kind {
                NodeKind::Declaration(d) => (d.prop.clone(), d.value.clone()),
                _ => return,
            },
            None => return,
        };

        let mut params = self.parse_value(&decl_value);
        // JS: `params = params.filter(i => { let prop = this.prefixes.remove[this.findProp(i)]; return !prop || !prop.remove })`
        params.retain(|p| {
            let prop = Self::find_prop(p);
            !self.prefixes.should_remove(&prop)
        });
        let value = self.stringify_params(&mut params);

        if decl_value == value {
            return;
        }

        // JS: `if (params.length === 0) { decl.remove(); return }`
        if params.is_empty() {
            self.remove_at(root, path);
            return;
        }

        // JS: `let double = decl.parent.some(i => i.prop === decl.prop && i.value === value)`
        let double = parent_some(root, path, |sibling| match &sibling.kind {
            NodeKind::Declaration(d) => d.prop == decl_prop && d.value == value,
            _ => false,
        });
        // JS: `let smaller = decl.parent.some(i => i !== decl && i.prop === decl.prop && i.value.length > value.length)`
        let parent = match path.split_last() {
            Some((idx, parent_path)) => match node_at_path(root, parent_path) {
                Some(p) => Some((p, *idx)),
                None => None,
            },
            None => None,
        };
        let smaller = match parent {
            Some((p, decl_idx)) => match p.nodes() {
                Some(siblings) => siblings.iter().enumerate().any(|(i, sibling)| {
                    i != decl_idx
                        && match &sibling.kind {
                            NodeKind::Declaration(d) => {
                                d.prop == decl_prop && d.value.len() > value.len()
                            }
                            _ => false,
                        }
                }),
                None => false,
            },
            None => false,
        };

        if double || smaller {
            self.remove_at(root, path);
            return;
        }

        // JS: `decl.value = value`
        if let Some(here) = node_at_path_mut(root, path) {
            if let NodeKind::Declaration(d) = &mut here.kind {
                d.value = value;
            }
        }
    }

    /// `decl.remove()` — splice this decl out of its parent.
    fn remove_at(&self, root: &mut Node, path: &[usize]) {
        let (idx, parent_path) = match path.split_last() {
            Some((i, p)) => (*i, p.to_vec()),
            None => return,
        };
        if let Some(parent) = node_at_path_mut(root, &parent_path) {
            if let Some(nodes) = parent.nodes_mut() {
                if idx < nodes.len() {
                    nodes.remove(idx);
                }
            }
        }
    }

    /// JS: `parse(value)`.
    /// ```js
    /// let ast = parser(value)
    /// let result = []
    /// let param = []
    /// for (let node of ast.nodes) {
    ///   param.push(node)
    ///   if (node.type === 'div' && node.value === ',') {
    ///     result.push(param); param = []
    ///   }
    /// }
    /// result.push(param)
    /// return result.filter(i => i.length > 0)
    /// ```
    pub fn parse_value(&self, value: &str) -> Vec<Vec<VNode>> {
        let nodes = vp_parse(value);
        let mut result: Vec<Vec<VNode>> = Vec::new();
        let mut param: Vec<VNode> = Vec::new();
        for node in nodes {
            let is_comma = node.kind == VNodeKind::Div && node.value == ",";
            param.push(node);
            if is_comma {
                result.push(std::mem::take(&mut param));
            }
        }
        result.push(param);
        result.retain(|p| !p.is_empty());
        result
    }

    /// JS: `stringify(params)`.
    /// ```js
    /// if (params.length === 0) return ''
    /// let nodes = []
    /// for (let param of params) {
    ///   if (param[param.length - 1].type !== 'div') param.push(this.div(params))
    ///   nodes = nodes.concat(param)
    /// }
    /// if (nodes[0].type === 'div') nodes = nodes.slice(1)
    /// if (nodes[nodes.length - 1].type === 'div') nodes = nodes.slice(0, -1)
    /// return parser.stringify({ nodes })
    /// ```
    /// **MUTATES `params`** — pushes a trailing div onto every param that
    /// doesn't already have one. Mirror JS exactly: subsequent calls
    /// (`cleanFromUnprefixed` etc.) observe the mutation.
    pub fn stringify_params(&self, params: &mut [Vec<VNode>]) -> String {
        if params.is_empty() {
            return String::new();
        }
        let div_template = Self::find_or_create_div(params);

        let mut nodes: Vec<VNode> = Vec::new();
        for param in params.iter_mut() {
            let last_is_div = param
                .last()
                .map(|n| n.kind == VNodeKind::Div)
                .unwrap_or(false);
            if !last_is_div {
                param.push(div_template.clone());
            }
            nodes.extend(param.iter().cloned());
        }
        if nodes
            .first()
            .map(|n| n.kind == VNodeKind::Div)
            .unwrap_or(false)
        {
            nodes.remove(0);
        }
        if nodes
            .last()
            .map(|n| n.kind == VNodeKind::Div)
            .unwrap_or(false)
        {
            nodes.pop();
        }
        vp_stringify(&nodes)
    }

    /// JS: `clone(origin, name, param)`.
    /// ```js
    /// let result = []
    /// let changed = false
    /// for (let i of param) {
    ///   if (!changed && i.type === 'word' && i.value === origin) {
    ///     result.push({ type: 'word', value: name }); changed = true
    ///   } else result.push(i)
    /// }
    /// return result
    /// ```
    /// JS pushes a NEW Word node `{ type: 'word', value: name }` with no
    /// `before`/`after`/`sourceIndex` — postcss-value-parser stringifies
    /// Words by emitting `node.value` only, so missing fields don't reach
    /// output bytes.
    pub fn clone_param(origin: &str, name: &str, param: &[VNode]) -> Vec<VNode> {
        let mut result: Vec<VNode> = Vec::with_capacity(param.len());
        let mut changed = false;
        for node in param {
            if !changed && node.kind == VNodeKind::Word && node.value == origin {
                result.push(VNode {
                    kind: VNodeKind::Word,
                    value: name.to_string(),
                    before: String::new(),
                    after: String::new(),
                    quote: None,
                    unclosed: false,
                    nodes: Vec::new(),
                    source_index: 0,
                    source_end_index: 0,
                });
                changed = true;
            } else {
                result.push(node.clone());
            }
        }
        result
    }

    /// JS: `div(params)` — find or create the comma separator.
    /// ```js
    /// for (let param of params) for (let node of param) {
    ///   if (node.type === 'div' && node.value === ',') return node
    /// }
    /// return { type: 'div', value: ',', after: ' ' }
    /// ```
    /// JS returns the SAME node reference and pushes it onto multiple
    /// params (sharing). Stringify reads each independently. We clone for
    /// owned-vec semantics — bytes are identical.
    fn find_or_create_div(params: &[Vec<VNode>]) -> VNode {
        for param in params {
            for node in param {
                if node.kind == VNodeKind::Div && node.value == "," {
                    return node.clone();
                }
            }
        }
        VNode {
            kind: VNodeKind::Div,
            value: ",".to_string(),
            before: String::new(),
            after: " ".to_string(),
            quote: None,
            unclosed: false,
            nodes: Vec::new(),
            source_index: 0,
            source_end_index: 0,
        }
    }

    /// JS: `cleanOtherPrefixes(params, prefix)`.
    /// ```js
    /// return params.filter(param => {
    ///   let current = vendor.prefix(this.findProp(param))
    ///   return current === '' || current === prefix
    /// })
    /// ```
    pub fn clean_other_prefixes(&self, params: &[Vec<VNode>], prefix: &str) -> Vec<Vec<VNode>> {
        params
            .iter()
            .filter(|p| {
                let prop = Self::find_prop(p);
                let current = vendor::prefix(&prop);
                current.is_empty() || current == prefix
            })
            .cloned()
            .collect()
    }

    /// JS: `cleanFromUnprefixed(params, prefix)`.
    /// ```js
    /// let remove = params
    ///   .map(i => this.findProp(i))
    ///   .filter(i => i.slice(0, prefix.length) === prefix)
    ///   .map(i => this.prefixes.unprefixed(i))
    ///
    /// let result = []
    /// for (let param of params) {
    ///   let prop = this.findProp(param)
    ///   let p = vendor.prefix(prop)
    ///   if (!remove.includes(prop) && (p === prefix || p === '')) {
    ///     result.push(param)
    ///   }
    /// }
    /// return result
    /// ```
    pub fn clean_from_unprefixed(&self, params: &[Vec<VNode>], prefix: &str) -> Vec<Vec<VNode>> {
        let remove: Vec<String> = params
            .iter()
            .map(|p| Self::find_prop(p))
            .filter(|prop| prop.starts_with(prefix))
            .map(|prop| self.prefixes.unprefixed(&prop))
            .collect();

        let mut result: Vec<Vec<VNode>> = Vec::new();
        for param in params {
            let prop = Self::find_prop(param);
            let p = vendor::prefix(&prop);
            if !remove.iter().any(|r| r == &prop) && (p == prefix || p.is_empty()) {
                result.push(param.clone());
            }
        }
        result
    }

    /// JS: `disabled(prop, prefix)`.
    /// ```js
    /// let other = ['order', 'justify-content', 'align-self', 'align-content']
    /// if (prop.includes('flex') || other.includes(prop)) {
    ///   if (this.prefixes.options.flexbox === false) return true
    ///   if (this.prefixes.options.flexbox === 'no-2009') return prefix.includes('2009')
    /// }
    /// return undefined
    /// ```
    pub fn disabled(&self, prop: &str, prefix: &str) -> bool {
        const OTHER: &[&str] = &["order", "justify-content", "align-self", "align-content"];
        if prop.contains("flex") || OTHER.contains(&prop) {
            match self.prefixes.flexbox() {
                FlexboxOption::Off => return true,
                FlexboxOption::No2009 => return prefix.contains("2009"),
                FlexboxOption::On => {}
            }
        }
        false
    }

    /// JS: `ruleVendorPrefixes(decl)`.
    /// ```js
    /// let { parent } = decl
    /// if (parent.type !== 'rule') return false
    /// else if (!parent.selector.includes(':-')) return false
    /// let selectors = Browsers.prefixes().filter(s =>
    ///   parent.selector.includes(':' + s)
    /// )
    /// return selectors.length > 0 ? selectors : false
    /// ```
    pub fn rule_vendor_prefixes(&self, root: &Node, path: &[usize]) -> Option<Vec<String>> {
        let parent_path = path.split_last().map(|(_, p)| p)?;
        let parent = node_at_path(root, parent_path)?;
        let selector = match &parent.kind {
            NodeKind::Rule(r) => &r.selector,
            _ => return None,
        };
        if !selector.contains(":-") {
            return None;
        }
        let matches: Vec<String> = Browsers::prefixes()
            .iter()
            .filter(|s| selector.contains(&format!(":{s}")))
            .cloned()
            .collect();
        if matches.is_empty() {
            None
        } else {
            Some(matches)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use postcss_core::{parse, stringify};

    /// Mock `TransitionPrefixesView` — drives tests without depending on
    /// `Prefixes::new` (AGENT_1's territory).
    #[derive(Debug, Default)]
    struct MockPrefixes {
        add: IndexMap<String, Vec<String>>,
        remove: IndexMap<String, bool>,
        flexbox: FlexboxOption,
    }

    impl MockPrefixes {
        fn with_add(mut self, prop: &str, prefixes: &[&str]) -> Self {
            self.add.insert(
                prop.to_string(),
                prefixes.iter().map(|s| s.to_string()).collect(),
            );
            self
        }

        fn with_remove(mut self, prefixed: &str) -> Self {
            self.remove.insert(prefixed.to_string(), true);
            self
        }
    }

    impl TransitionPrefixesView for MockPrefixes {
        fn add_prefixes(&self, prop: &str) -> Option<&[String]> {
            self.add.get(prop).map(|v| v.as_slice())
        }
        fn should_remove(&self, prop: &str) -> bool {
            self.remove.get(prop).copied().unwrap_or(false)
        }
        fn prefixed(&self, prop: &str, prefix: &str) -> String {
            // Default Declaration::prefixed is `prefix + prop`. We mirror
            // that for tests; hacks would override at the Prefixes level.
            let unprefixed = vendor::unprefixed(prop);
            format!("{prefix}{unprefixed}")
        }
        fn unprefixed(&self, prop: &str) -> String {
            vendor::unprefixed(prop)
        }
        fn flexbox(&self) -> FlexboxOption {
            self.flexbox
        }
    }

    fn first_decl_path() -> Vec<usize> {
        // root → rule(0) → decl(0).
        vec![0, 0]
    }

    // --------------------- find_prop ---------------------

    #[test]
    fn find_prop_returns_first_word() {
        let nodes = vp_parse("transform 0.3s ease");
        assert_eq!(Transition::find_prop(&nodes), "transform");
    }

    #[test]
    fn find_prop_handles_leading_duration() {
        // `0.3s transform ease` — first token is `0.3s` (Word, starts with
        // digit). JS scans for next Word token (skipping Space at i=1).
        let nodes = vp_parse("0.3s transform ease");
        assert_eq!(Transition::find_prop(&nodes), "transform");
    }

    #[test]
    fn find_prop_falls_back_to_first_value_when_no_word_after() {
        // Pure `0.3s` with no word after. JS returns the first value as-is.
        let nodes = vp_parse("0.3s");
        assert_eq!(Transition::find_prop(&nodes), "0.3s");
    }

    // --------------------- parse / stringify ---------------------

    #[test]
    fn parse_value_splits_on_comma_div() {
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        let params = t.parse_value("transform 0.3s, opacity 0.5s");
        assert_eq!(params.len(), 2);
        // First param ends with the comma div (JS behavior: param.push(node)
        // happens BEFORE the result.push split).
        let last = params[0].last().unwrap();
        assert_eq!(last.kind, VNodeKind::Div);
        assert_eq!(last.value, ",");
    }

    #[test]
    fn parse_value_filters_empty_params() {
        // Trailing comma → empty trailing param after the split, filtered.
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        let params = t.parse_value("transform 0.3s,");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn stringify_params_empty_returns_empty_string() {
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        let mut params: Vec<Vec<VNode>> = Vec::new();
        assert_eq!(t.stringify_params(&mut params), "");
    }

    #[test]
    fn stringify_params_round_trips_simple_value() {
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        let mut params = t.parse_value("transform 0.3s ease");
        assert_eq!(t.stringify_params(&mut params), "transform 0.3s ease");
    }

    #[test]
    fn stringify_params_adds_trailing_div_to_each() {
        // Two params, neither has a trailing div. After stringify, both
        // have one (mirrors JS in-place push). Verifying the mutation
        // side-effect because `cleanFromUnprefixed` depends on it.
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        // Build raw params bypassing parse, so neither has a trailing div.
        let p1 = vec![VNode {
            kind: VNodeKind::Word,
            value: "transform".to_string(),
            before: String::new(),
            after: String::new(),
            quote: None,
            unclosed: false,
            nodes: Vec::new(),
            source_index: 0,
            source_end_index: 9,
        }];
        let p2 = vec![VNode {
            kind: VNodeKind::Word,
            value: "opacity".to_string(),
            before: String::new(),
            after: String::new(),
            quote: None,
            unclosed: false,
            nodes: Vec::new(),
            source_index: 0,
            source_end_index: 7,
        }];
        let mut params = vec![p1, p2];
        let _ = t.stringify_params(&mut params);
        assert_eq!(params[0].last().unwrap().kind, VNodeKind::Div);
        assert_eq!(params[1].last().unwrap().kind, VNodeKind::Div);
    }

    // --------------------- clone_param ---------------------

    #[test]
    fn clone_param_replaces_first_matching_word() {
        let nodes = vp_parse("transform 0.3s ease");
        let cloned = Transition::clone_param("transform", "-webkit-transform", &nodes);
        assert_eq!(cloned.first().unwrap().value, "-webkit-transform");
        // Subsequent tokens unchanged.
        assert_eq!(cloned[2].value, "0.3s");
    }

    #[test]
    fn clone_param_only_replaces_once() {
        let nodes = vp_parse("transform transform");
        let cloned = Transition::clone_param("transform", "-webkit-transform", &nodes);
        assert_eq!(cloned[0].value, "-webkit-transform");
        // Second `transform` stays as-is.
        assert_eq!(cloned[2].value, "transform");
    }

    // --------------------- clean_from_unprefixed ---------------------

    #[test]
    fn clean_from_unprefixed_drops_unprefixed_when_prefixed_present() {
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        let mut params: Vec<Vec<VNode>> = Vec::new();
        params.extend(t.parse_value("transform 0.3s, -webkit-transform 0.3s"));
        let filtered = t.clean_from_unprefixed(&params, "-webkit-");
        // Only the -webkit-transform param survives.
        assert_eq!(filtered.len(), 1);
        let prop = Transition::find_prop(&filtered[0]);
        assert_eq!(prop, "-webkit-transform");
    }

    #[test]
    fn clean_from_unprefixed_drops_other_vendor_prefixes() {
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        let params = t.parse_value("transform 0.3s, -moz-transform 0.3s");
        let filtered = t.clean_from_unprefixed(&params, "-webkit-");
        // -moz-transform dropped; transform kept (no -webkit- in params, so
        // remove array is empty; transform's vendor prefix is '' → kept).
        assert_eq!(filtered.len(), 1);
        let prop = Transition::find_prop(&filtered[0]);
        assert_eq!(prop, "transform");
    }

    // --------------------- clean_other_prefixes ---------------------

    #[test]
    fn clean_other_prefixes_keeps_unprefixed_and_matching_prefix() {
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        let params = t.parse_value("transform 0.3s, -webkit-transform 0.3s, -moz-transform 0.3s");
        let filtered = t.clean_other_prefixes(&params, "-webkit-");
        // Should keep `transform` (empty prefix) and `-webkit-transform`.
        assert_eq!(filtered.len(), 2);
    }

    // --------------------- disabled ---------------------

    #[test]
    fn disabled_off_drops_flexbox_props() {
        let mock = MockPrefixes {
            flexbox: FlexboxOption::Off,
            ..MockPrefixes::default()
        };
        let t = Transition::new(&mock);
        assert!(t.disabled("flex", "-webkit-"));
        assert!(t.disabled("order", "-webkit-"));
        assert!(t.disabled("justify-content", "-webkit-"));
        assert!(!t.disabled("transform", "-webkit-"));
    }

    #[test]
    fn disabled_no_2009_only_drops_2009_prefix() {
        let mock = MockPrefixes {
            flexbox: FlexboxOption::No2009,
            ..MockPrefixes::default()
        };
        let t = Transition::new(&mock);
        assert!(t.disabled("flex", "-webkit- 2009"));
        assert!(!t.disabled("flex", "-webkit-"));
    }

    #[test]
    fn disabled_default_off() {
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        assert!(!t.disabled("flex", "-webkit-"));
        assert!(!t.disabled("transform", "-webkit-"));
    }

    // --------------------- rule_vendor_prefixes ---------------------

    #[test]
    fn rule_vendor_prefixes_returns_none_for_plain_selector() {
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        let r = parse("a { transition: transform 0.3s; }").unwrap();
        assert!(t.rule_vendor_prefixes(&r.root, &first_decl_path()).is_none());
    }

    #[test]
    fn rule_vendor_prefixes_detects_vendor_pseudo() {
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        let r = parse(":-webkit-full-screen { transition: transform 0.3s; }")
            .unwrap();
        let res = t.rule_vendor_prefixes(&r.root, &first_decl_path());
        assert!(res.is_some());
        let prefixes = res.unwrap();
        assert!(prefixes.iter().any(|p| p == "-webkit-"));
    }

    // --------------------- add ---------------------

    #[test]
    fn add_no_prefixed_props_is_noop() {
        // `transform` has no prefixes in the mock, so nothing should change.
        let mock = MockPrefixes::default();
        let t = Transition::new(&mock);
        let css = "a { transition: transform 0.3s ease; }";
        let mut r = parse(css).unwrap();
        let mut warnings: Vec<String> = Vec::new();
        t.add(&mut r.root, &first_decl_path(), &mut warnings);
        let out = stringify(&r);
        assert_eq!(out, css);
        assert!(warnings.is_empty());
    }

    #[test]
    fn add_inserts_webkit_sibling_for_transform_value() {
        // The mock declares `transform → ['-webkit-']`. The transition
        // value should gain a -webkit-transform clone in the value list,
        // and the decl's siblings should include both an unprefixed
        // fallback and the modified original. This exercises the cursor-
        // shift bump (≥2 inserts on the same decl).
        let mock = MockPrefixes::default().with_add("transform", &["-webkit-"]);
        let t = Transition::new(&mock);
        let mut r = parse("a { transition: transform 0.3s ease; }").unwrap();
        let mut warnings: Vec<String> = Vec::new();
        t.add(&mut r.root, &first_decl_path(), &mut warnings);
        let out = stringify(&r);
        // The value list now contains both transform and -webkit-transform.
        assert!(
            out.contains("transform 0.3s ease, -webkit-transform 0.3s ease"),
            "expected combined value list in: {out}"
        );
        // A `transition: -webkit-transform 0.3s ease` fallback was inserted.
        assert!(
            out.contains("transition: -webkit-transform 0.3s ease"),
            "expected webkit-only fallback decl in: {out}"
        );
    }

    #[test]
    fn add_inserts_webkit_transition_when_decl_has_webkit() {
        // declPrefixes includes -webkit- → first cloneBefore inserts
        // `-webkit-transition` decl.
        let mock = MockPrefixes::default()
            .with_add("transition", &["-webkit-"])
            .with_add("transform", &["-webkit-"]);
        let t = Transition::new(&mock);
        let mut r = parse("a { transition: transform 0.3s ease; }").unwrap();
        let mut warnings: Vec<String> = Vec::new();
        t.add(&mut r.root, &first_decl_path(), &mut warnings);
        let out = stringify(&r);
        assert!(
            out.contains("-webkit-transition: -webkit-transform 0.3s ease"),
            "expected -webkit-transition decl in: {out}"
        );
    }

    #[test]
    fn add_skips_when_value_already_prefixed() {
        // names contains a prop starting with '-' → JS returns early.
        let mock = MockPrefixes::default().with_add("transform", &["-webkit-"]);
        let t = Transition::new(&mock);
        let css = "a { transition: -webkit-transform 0.3s ease; }";
        let mut r = parse(css).unwrap();
        let mut warnings: Vec<String> = Vec::new();
        t.add(&mut r.root, &first_decl_path(), &mut warnings);
        // No change — value already starts with `-`, JS bails.
        let out = stringify(&r);
        assert_eq!(out, css);
    }

    #[test]
    fn add_skips_ms_transform_prefix() {
        // `-ms-transform` is explicitly excluded by JS:
        //   `if (prefixed !== '-ms-transform' && !names.includes(prefixed))`.
        // With ONLY -ms- as the available prefix for transform, no clone
        // should be added → no value change → no inserts.
        let mock = MockPrefixes::default().with_add("transform", &["-ms-"]);
        let t = Transition::new(&mock);
        let css = "a { transition: transform 0.3s ease; }";
        let mut r = parse(css).unwrap();
        let mut warnings: Vec<String> = Vec::new();
        t.add(&mut r.root, &first_decl_path(), &mut warnings);
        let out = stringify(&r);
        assert_eq!(out, css);
    }

    #[test]
    fn add_handles_two_prefixes_with_cursor_shift() {
        // ≥2 prefixes on the same decl → exercises the cursor-shift bump.
        // A bug here silently drops the second insert into the wrong slot.
        let mock = MockPrefixes::default()
            .with_add("transition", &["-webkit-", "-o-"])
            .with_add("transform", &["-webkit-", "-o-"]);
        let t = Transition::new(&mock);
        let mut r = parse("a { transition: transform 0.3s ease; }").unwrap();
        let mut warnings: Vec<String> = Vec::new();
        t.add(&mut r.root, &first_decl_path(), &mut warnings);
        let out = stringify(&r);
        // Both prefixed transition decls must land.
        assert!(out.contains("-webkit-transition:"), "expected -webkit-transition in: {out}");
        assert!(out.contains("-o-transition:"), "expected -o-transition in: {out}");
    }

    // --------------------- remove ---------------------

    #[test]
    fn remove_drops_marked_param_and_updates_value() {
        // Mock marks `-webkit-transform` for removal.
        let mock = MockPrefixes::default().with_remove("-webkit-transform");
        let t = Transition::new(&mock);
        let mut r = parse(
            "a { transition: transform 0.3s ease, -webkit-transform 0.3s ease; }",
        )
        .unwrap();
        t.remove(&mut r.root, &first_decl_path());
        let out = stringify(&r);
        assert!(out.contains("transform 0.3s ease"));
        assert!(!out.contains("-webkit-transform 0.3s ease"));
    }

    #[test]
    fn remove_removes_decl_when_all_params_filtered() {
        // Every param in the value matches a remove key → params.length=0
        // → JS calls decl.remove(). The whole decl should disappear.
        let mock = MockPrefixes::default().with_remove("-webkit-transform");
        let t = Transition::new(&mock);
        let mut r =
            parse("a { transition: -webkit-transform 0.3s ease; }").unwrap();
        t.remove(&mut r.root, &first_decl_path());
        let out = stringify(&r);
        assert!(!out.contains("transition"), "expected decl removed: {out}");
    }

    // --------------------- check_for_warning ---------------------

    #[test]
    fn check_for_warning_emits_when_mixed_transition_property() {
        // `transition-property: transform` (transform IS prefixed) AND
        // sibling `transition-duration: 0.3s, 0.5s` (multi-value) →
        // JS emits a warning.
        let mock = MockPrefixes::default().with_add("transform", &["-webkit-"]);
        let t = Transition::new(&mock);
        let r = parse(
            "a { transition-property: transform; transition-duration: 0.3s, 0.5s; }",
        )
        .unwrap();
        let mut warnings: Vec<String> = Vec::new();
        // Path to the `transition-property` decl (index 0 within rule).
        t.check_for_warning(&r.root, &first_decl_path(), &mut warnings);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Replace transition-property"));
    }

    #[test]
    fn check_for_warning_silent_for_transition_shorthand() {
        // checkForWarning bails immediately when decl.prop != 'transition-property'.
        let mock = MockPrefixes::default().with_add("transform", &["-webkit-"]);
        let t = Transition::new(&mock);
        let r = parse("a { transition: transform 0.3s ease; }").unwrap();
        let mut warnings: Vec<String> = Vec::new();
        t.check_for_warning(&r.root, &first_decl_path(), &mut warnings);
        assert!(warnings.is_empty());
    }
}
