//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/processor.js`.
//!
//! ## Slice landed in this AGENT_4 pass
//!
//! - Module-level constants (regexes + `SIZES` list) — byte-clean.
//! - `Processor` struct + `Processor::new`.
//! - The orchestrator-control helpers (pure logic, used by the eventual
//!   `add` / `remove` walks):
//!   - `disabled`, `disabled_decl`, `disabled_value`
//!   - `grid_status`, `display_type`, `with_hack_value`, `reduce_spaces`
//! - `GridStatus` and `DisplayType` enums for the tri-state JS returns.
//!
//! ## Slice deferred to the next AGENT_4 session
//!
//! - The main `add(css, result)` walk: `walkAtRules` → keyframes /
//!   viewport / `@supports` / resolution dispatch; `walkRules` →
//!   `prefixes.add.selectors[*].process(rule, result)`; `walkDecls` → the
//!   13-prop warning ladder + per-prop prefixer dispatch + `Value.save`.
//! - The main `remove(css, result)` walk.
//! - `insertAreas` (grid-area helper from `lib/hacks/grid-utils.js`).
//! - `preprocess()` — turns `Prefixes::add_table` (post-`select()`) into
//!   the per-bucket Prefixer-instance dispatch table JS calls
//!   `prefixes.add[prop].process(decl)` against. AGENT_5 territory but
//!   consumed here.
//!
//! Rationale for the slice: the helpers are pure-logic and depend only
//! on the AST + `_autoprefixer*` attr keys + `Prefixes::group` (which
//! AGENT_1 landed). They land 0→100% byte-clean today and are
//! load-bearing for both walks. The walks themselves require a
//! Prefixer-instance dispatch table that doesn't exist; landing the
//! walks against ad-hoc dispatch would either:
//! 1. fork from JS shape (drift), or
//! 2. require introducing new fields on `Prefixes` (AGENT_1 territory).
//!
//! Per AGENT_4.md "Scope discipline": one slice 0→100% byte-clean.

use once_cell::sync::Lazy;
use postcss_core::{node_at_path, node_at_path_mut, AttrValue, Node, NodeKind};
use regex::Regex;

use crate::prefixes::Prefixes;

// JS top-of-file regex constants (lines 6-9 of processor.js). Keep
// raw-string + flags identical to JS for byte parity.
//
// `OLD_LINEAR` and `OLD_RADIAL` drive the gradient-syntax warnings inside
// the `walkDecls` decl pass — that pass is deferred to the next AGENT_4
// session, so they're unused at the moment. Pre-landing them keeps the
// module-level constants in JS source-order so the next session doesn't
// re-derive the patterns.
#[allow(dead_code)]
pub(crate) static OLD_LINEAR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(^|[^-])linear-gradient\(\s*(top|left|right|bottom)").unwrap()
});
#[allow(dead_code)]
pub(crate) static OLD_RADIAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(^|[^-])radial-gradient\(\s*\d+(\w*|%)\s+\d+(\w*|%)\s*,").unwrap()
});
static IGNORE_NEXT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(!\s*)?autoprefixer:\s*ignore\s+next").unwrap());
static GRID_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(!\s*)?autoprefixer\s*grid:\s*(on|off|(no-)?autoplace)").unwrap()
});

// The on/off control-comment regex used by `disabled` for body comments.
static CONTROL_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(!\s*)?autoprefixer:\s*(off|on)").unwrap());

// Used by `disabled` to decide whether the comment text is "on".
static ON_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)on").unwrap());

// JS `gridStatus` parses `: autoplace` / `no-autoplace` / `on` from the
// matched control comment. We reuse `GRID_REGEX` for matching, then
// inspect the text manually with these helpers.
static AUTOPLACE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i):\s*autoplace").unwrap());
static NO_AUTOPLACE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)no-autoplace").unwrap());

/// JS `SIZES` constant (lines 11-24 of processor.js). Drives the
/// `walkDecls` "fill / fill-available" warning ladder. Public so the
/// next AGENT_4 session can reuse it from the walk.
pub const SIZES: &[&str] = &[
    "width",
    "height",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
    "inline-size",
    "min-inline-size",
    "max-inline-size",
    "block-size",
    "min-block-size",
    "max-block-size",
];

/// Per-node cache key for `disabled()` answers. JS:
/// `node._autoprefixerDisabled`.
pub const ATTR_DISABLED: &str = "_autoprefixerDisabled";
/// Per-node flag set by `disabled()` when the node was disabled by a
/// preceding `autoprefixer: ignore next` comment, NOT by an enclosing
/// `autoprefixer: off` block. JS: `node._autoprefixerSelfDisabled`.
pub const ATTR_SELF_DISABLED: &str = "_autoprefixerSelfDisabled";
/// Per-node cache key for `gridStatus()` answers. JS:
/// `node._autoprefixerGridStatus`. Encoded as `Bool(false|true)` for the
/// JS `false` / `true` cases, `String("autoplace")` for the JS
/// `'autoplace'` case.
pub const ATTR_GRID_STATUS: &str = "_autoprefixerGridStatus";

/// JS `gridStatus` returns `false | true | 'autoplace'`. Modelled
/// explicitly so callers can pattern-match without re-parsing strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridStatus {
    Off,
    On,
    Autoplace,
}

impl GridStatus {
    /// `false` in JS truthiness.
    pub fn is_off(self) -> bool {
        matches!(self, GridStatus::Off)
    }

    /// JS truthy — `true` and `'autoplace'` both truthy.
    #[allow(dead_code)]
    pub fn is_truthy(self) -> bool {
        !self.is_off()
    }

    fn from_attr(v: &AttrValue) -> Option<Self> {
        match v {
            AttrValue::Bool(false) => Some(GridStatus::Off),
            AttrValue::Bool(true) => Some(GridStatus::On),
            AttrValue::String(s) if s == "autoplace" => Some(GridStatus::Autoplace),
            _ => None,
        }
    }

    fn to_attr(self) -> AttrValue {
        match self {
            GridStatus::Off => AttrValue::Bool(false),
            GridStatus::On => AttrValue::Bool(true),
            GridStatus::Autoplace => AttrValue::String("autoplace".to_string()),
        }
    }
}

/// JS `displayType(decl)` returns `'flex' | 'grid' | false`. Modelled
/// explicitly so callers don't string-compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayType {
    /// JS `false` — no enclosing `display: flex` or `display: grid`.
    None,
    Flex,
    Grid,
}

/// JS `class Processor`. Holds a borrowed `Prefixes` for the session.
///
/// The walk methods (`add` / `remove`) are deferred to the next AGENT_4
/// session — see module docs. The helpers below are independent of the
/// walks and are reachable now.
pub struct Processor<'a> {
    pub prefixes: &'a Prefixes,
}

impl<'a> Processor<'a> {
    /// JS: `constructor(prefixes) { this.prefixes = prefixes }`.
    pub fn new(prefixes: &'a Prefixes) -> Self {
        Self { prefixes }
    }

    /// JS: `disabled(node, result)`.
    /// ```js
    /// disabled(node, result) {
    ///   if (!node) return false
    ///   if (node._autoprefixerDisabled !== undefined) return node._autoprefixerDisabled
    ///   if (node.parent) {
    ///     let p = node.prev()
    ///     if (p && p.type === 'comment' && IGNORE_NEXT.test(p.text)) {
    ///       node._autoprefixerDisabled = true
    ///       node._autoprefixerSelfDisabled = true
    ///       return true
    ///     }
    ///   }
    ///   let value = null
    ///   if (node.nodes) {
    ///     let status
    ///     node.each(i => {
    ///       if (i.type !== 'comment') return
    ///       if (/(!\s*)?autoprefixer:\s*(off|on)/i.test(i.text)) {
    ///         if (typeof status !== 'undefined') {
    ///           result.warn('Second Autoprefixer control comment was ignored. ...')
    ///         } else {
    ///           status = /on/i.test(i.text)
    ///         }
    ///       }
    ///     })
    ///     if (status !== undefined) value = !status
    ///   }
    ///   if (!node.nodes || value === null) {
    ///     if (node.parent) {
    ///       let isParentDisabled = this.disabled(node.parent, result)
    ///       if (node.parent._autoprefixerSelfDisabled === true) value = false
    ///       else value = isParentDisabled
    ///     } else {
    ///       value = false
    ///     }
    ///   }
    ///   node._autoprefixerDisabled = value
    ///   return value
    /// }
    /// ```
    pub fn disabled(
        &self,
        root: &mut Node,
        path: &[usize],
        warnings: &mut Vec<String>,
    ) -> bool {
        // Cache hit?
        if let Some(here) = node_at_path(root, path) {
            if let Some(b) = here.attrs.get_bool(ATTR_DISABLED) {
                return b;
            }
        } else {
            return false;
        }

        // JS: `if (node.parent) { let p = node.prev() ... }`.
        // `prev()` is the sibling at parent_index - 1.
        if !path.is_empty() {
            let parent_idx = *path.last().unwrap();
            if parent_idx > 0 {
                let parent = path.len().saturating_sub(1);
                let prev_path: Vec<usize> = {
                    let mut p = path[..parent].to_vec();
                    p.push(parent_idx - 1);
                    p
                };
                let ignore_next_hit = match node_at_path(root, &prev_path) {
                    Some(prev) => match &prev.kind {
                        NodeKind::Comment(c) => IGNORE_NEXT.is_match(&c.text),
                        _ => false,
                    },
                    None => false,
                };
                if ignore_next_hit {
                    if let Some(here) = node_at_path_mut(root, path) {
                        here.attrs.set(ATTR_DISABLED, AttrValue::Bool(true));
                        here.attrs.set(ATTR_SELF_DISABLED, AttrValue::Bool(true));
                    }
                    return true;
                }
            }
        }

        // Container case: scan child comments for `autoprefixer: on/off`.
        let (is_container, child_status) = {
            let here = match node_at_path(root, path) {
                Some(n) => n,
                None => return false,
            };
            if here.kind.is_container() {
                let mut status: Option<bool> = None;
                if let Some(children) = here.nodes() {
                    for child in children {
                        if let NodeKind::Comment(c) = &child.kind {
                            if CONTROL_REGEX.is_match(&c.text) {
                                if status.is_some() {
                                    warnings.push(
                                        "Second Autoprefixer control comment \
was ignored. Autoprefixer applies control \
comment to whole block, not to next rules."
                                            .to_string(),
                                    );
                                } else {
                                    status = Some(ON_REGEX.is_match(&c.text));
                                }
                            }
                        }
                    }
                }
                (true, status)
            } else {
                (false, None)
            }
        };

        // JS `value`: null | bool. Encoded as Option<bool>.
        let mut value: Option<bool> = None;
        if is_container {
            if let Some(s) = child_status {
                // JS: `value = !status` — `on` → enabled (false=not disabled),
                // `off` → disabled (true).
                value = Some(!s);
            }
        }

        // JS: `if (!node.nodes || value === null) { ... parent recurse }`.
        if !is_container || value.is_none() {
            if !path.is_empty() {
                let parent_path = path[..path.len() - 1].to_vec();
                let parent_disabled = self.disabled(root, &parent_path, warnings);
                let parent_self_disabled = node_at_path(root, &parent_path)
                    .map(|p| {
                        p.attrs.get_bool(ATTR_SELF_DISABLED) == Some(true)
                    })
                    .unwrap_or(false);
                if parent_self_disabled {
                    value = Some(false);
                } else {
                    value = Some(parent_disabled);
                }
            } else {
                value = Some(false);
            }
        }

        let final_value = value.unwrap_or(false);
        if let Some(here) = node_at_path_mut(root, path) {
            here.attrs.set(ATTR_DISABLED, AttrValue::Bool(final_value));
        }
        final_value
    }

    /// JS: `disabledDecl(node, result)`.
    /// ```js
    /// disabledDecl(node, result) {
    ///   if (this.gridStatus(node, result) === false && node.type === 'decl') {
    ///     if (node.prop.includes('grid') || node.prop === 'justify-items') return true
    ///   }
    ///   if (this.prefixes.options.flexbox === false && node.type === 'decl') {
    ///     let other = ['order', 'justify-content', 'align-items', 'align-content']
    ///     if (node.prop.includes('flex') || other.includes(node.prop)) return true
    ///   }
    ///   return this.disabled(node, result)
    /// }
    /// ```
    ///
    /// **Flexbox-false branch caveat:** JS `options.flexbox === false`
    /// has no representation in the current `PrefixesOptions::flexbox:
    /// Option<String>` (see AGENT_2_DONE.md "Asks for AGENT_1"). The
    /// branch never fires today; this matches AGENT_2's `Supports::disabled`
    /// behaviour, so the gap is consistent across the codebase rather than
    /// localized to one site. When AGENT_1 ships the `FlexboxOption` enum,
    /// the branch wakes up here AND in `Supports::disabled` together.
    pub fn disabled_decl(
        &self,
        root: &mut Node,
        path: &[usize],
        warnings: &mut Vec<String>,
    ) -> bool {
        // Snapshot prop before borrowing root mutably for gridStatus.
        let decl_prop: Option<String> = match node_at_path(root, path) {
            Some(n) => match &n.kind {
                NodeKind::Declaration(d) => Some(d.prop.clone()),
                _ => None,
            },
            None => None,
        };

        if let Some(ref prop) = decl_prop {
            let grid = self.grid_status(root, path, warnings);
            if grid.is_off() && (prop.contains("grid") || prop == "justify-items") {
                return true;
            }
        }

        // Flexbox branch: see caveat above. Currently dormant.

        self.disabled(root, path, warnings)
    }

    /// JS: `disabledValue(node, result)`.
    /// ```js
    /// disabledValue(node, result) {
    ///   if (this.gridStatus(node, result) === false && node.type === 'decl') {
    ///     if (node.prop === 'display' && node.value.includes('grid')) return true
    ///   }
    ///   if (this.prefixes.options.flexbox === false && node.type === 'decl') {
    ///     if (node.prop === 'display' && node.value.includes('flex')) return true
    ///   }
    ///   if (node.type === 'decl' && node.prop === 'content') return true
    ///   return this.disabled(node, result)
    /// }
    /// ```
    pub fn disabled_value(
        &self,
        root: &mut Node,
        path: &[usize],
        warnings: &mut Vec<String>,
    ) -> bool {
        // Snapshot decl's prop+value once so the `&mut` for gridStatus
        // doesn't conflict.
        let decl_pv: Option<(String, String)> = match node_at_path(root, path) {
            Some(n) => match &n.kind {
                NodeKind::Declaration(d) => Some((d.prop.clone(), d.value.clone())),
                _ => None,
            },
            None => None,
        };

        if let Some((prop, value)) = &decl_pv {
            let grid = self.grid_status(root, path, warnings);
            if grid.is_off() && prop == "display" && value.contains("grid") {
                return true;
            }
            // Flexbox branch dormant — see disabled_decl caveat.
            if prop == "content" {
                return true;
            }
        }

        self.disabled(root, path, warnings)
    }

    /// JS: `gridStatus(node, result)`.
    /// ```js
    /// gridStatus(node, result) {
    ///   if (!node) return false
    ///   if (node._autoprefixerGridStatus !== undefined) return node._autoprefixerGridStatus
    ///   let value = null
    ///   if (node.nodes) {
    ///     let status
    ///     node.each(i => {
    ///       if (i.type !== 'comment') return
    ///       if (GRID_REGEX.test(i.text)) {
    ///         let hasAutoplace = /:\s*autoplace/i.test(i.text)
    ///         let noAutoplace = /no-autoplace/i.test(i.text)
    ///         if (typeof status !== 'undefined') {
    ///           result.warn('Second Autoprefixer grid control comment was ignored. ...')
    ///         } else if (hasAutoplace) status = 'autoplace'
    ///         else if (noAutoplace) status = true
    ///         else status = /on/i.test(i.text)
    ///       }
    ///     })
    ///     if (status !== undefined) value = status
    ///   }
    ///   if (node.type === 'atrule' && node.name === 'supports') {
    ///     let params = node.params
    ///     if (params.includes('grid') && params.includes('auto')) value = false
    ///   }
    ///   if (!node.nodes || value === null) {
    ///     if (node.parent) {
    ///       let isParentGrid = this.gridStatus(node.parent, result)
    ///       if (node.parent._autoprefixerSelfDisabled === true) value = false
    ///       else value = isParentGrid
    ///     } else if (typeof this.prefixes.options.grid !== 'undefined') {
    ///       value = this.prefixes.options.grid
    ///     } else if (typeof process.env.AUTOPREFIXER_GRID !== 'undefined') {
    ///       if (process.env.AUTOPREFIXER_GRID === 'autoplace') value = 'autoplace'
    ///       else value = true
    ///     } else {
    ///       value = false
    ///     }
    ///   }
    ///   node._autoprefixerGridStatus = value
    ///   return value
    /// }
    /// ```
    ///
    /// JS quirk: the JS function STORES `'autoplace'` as the literal
    /// string in the cache, and `true`/`false` as booleans. Pattern-match
    /// callers consuming the cached value should expect the tri-state.
    pub fn grid_status(
        &self,
        root: &mut Node,
        path: &[usize],
        warnings: &mut Vec<String>,
    ) -> GridStatus {
        // Cache hit?
        if let Some(here) = node_at_path(root, path) {
            if let Some(v) = here.attrs.get(ATTR_GRID_STATUS) {
                if let Some(g) = GridStatus::from_attr(v) {
                    return g;
                }
            }
        } else {
            return GridStatus::Off;
        }

        // Container case: scan child comments for `autoprefixer grid:` directives.
        let (is_container, child_status, is_supports_grid_auto) = {
            let here = match node_at_path(root, path) {
                Some(n) => n,
                None => return GridStatus::Off,
            };
            let supports_special = match &here.kind {
                NodeKind::AtRule(at) if at.name == "supports" => {
                    at.params.contains("grid") && at.params.contains("auto")
                }
                _ => false,
            };
            if here.kind.is_container() {
                let mut status: Option<GridStatus> = None;
                if let Some(children) = here.nodes() {
                    for child in children {
                        if let NodeKind::Comment(c) = &child.kind {
                            if GRID_REGEX.is_match(&c.text) {
                                let has_autoplace =
                                    AUTOPLACE_REGEX.is_match(&c.text);
                                let no_autoplace =
                                    NO_AUTOPLACE_REGEX.is_match(&c.text);
                                if status.is_some() {
                                    warnings.push(
                                        "Second Autoprefixer grid control comment was \
ignored. Autoprefixer applies control comments to the whole \
block, not to the next rules."
                                            .to_string(),
                                    );
                                } else if has_autoplace {
                                    status = Some(GridStatus::Autoplace);
                                } else if no_autoplace {
                                    status = Some(GridStatus::On);
                                } else if ON_REGEX.is_match(&c.text) {
                                    status = Some(GridStatus::On);
                                } else {
                                    status = Some(GridStatus::Off);
                                }
                            }
                        }
                    }
                }
                (true, status, supports_special)
            } else {
                (false, None, supports_special)
            }
        };

        let mut value: Option<GridStatus> = child_status;

        // JS: `if (node.type === 'atrule' && node.name === 'supports') { ... }`
        // — overrides any container child-comment status when the params
        // mention `grid` AND `auto` (the JS expression sets `value =
        // false` unconditionally on hit, regardless of any prior status).
        if is_supports_grid_auto {
            value = Some(GridStatus::Off);
        }

        // JS: `if (!node.nodes || value === null) { ... }`.
        if !is_container || value.is_none() {
            if !path.is_empty() {
                let parent_path = path[..path.len() - 1].to_vec();
                let parent_grid = self.grid_status(root, &parent_path, warnings);
                let parent_self_disabled = node_at_path(root, &parent_path)
                    .map(|p| {
                        p.attrs.get_bool(ATTR_SELF_DISABLED) == Some(true)
                    })
                    .unwrap_or(false);
                if parent_self_disabled {
                    value = Some(GridStatus::Off);
                } else {
                    value = Some(parent_grid);
                }
            } else if let Some(g) = self.prefixes.options.grid.as_deref() {
                // JS: `typeof options.grid !== 'undefined'`.
                // The string is whatever the JS user passed —
                // `true` / `false` / `'autoplace'` / `'no-autoplace'`. The
                // current Rust `PrefixesOptions::grid: Option<String>`
                // collapses true/false to None vs Some(...), so this
                // branch fires only when the user explicitly set the
                // option. Replicate JS literal coercion:
                //   "autoplace" → Autoplace
                //   "no-autoplace" → On
                //   "false" → Off
                //   anything else (incl. "true") → On
                value = Some(match g {
                    "autoplace" => GridStatus::Autoplace,
                    "false" => GridStatus::Off,
                    _ => GridStatus::On,
                });
            } else if let Ok(env) = std::env::var("AUTOPREFIXER_GRID") {
                // JS: `typeof process.env.AUTOPREFIXER_GRID !== 'undefined'`.
                value = Some(if env == "autoplace" {
                    GridStatus::Autoplace
                } else {
                    GridStatus::On
                });
            } else {
                value = Some(GridStatus::Off);
            }
        }

        let final_value = value.unwrap_or(GridStatus::Off);
        if let Some(here) = node_at_path_mut(root, path) {
            here.attrs.set(ATTR_GRID_STATUS, final_value.to_attr());
        }
        final_value
    }

    /// JS: `displayType(decl)`.
    /// ```js
    /// displayType(decl) {
    ///   for (let i of decl.parent.nodes) {
    ///     if (i.prop !== 'display') continue
    ///     if (i.value.includes('flex')) return 'flex'
    ///     if (i.value.includes('grid')) return 'grid'
    ///   }
    ///   return false
    /// }
    /// ```
    ///
    /// JS quirk: scans LEFT TO RIGHT and returns on the first matching
    /// `display` decl. If a parent has both `display: flex` and `display:
    /// grid` siblings (unusual but legal), only the first is honoured.
    pub fn display_type(&self, root: &Node, path: &[usize]) -> DisplayType {
        if path.is_empty() {
            return DisplayType::None;
        }
        let parent = match node_at_path(root, &path[..path.len() - 1]) {
            Some(p) => p,
            None => return DisplayType::None,
        };
        let nodes = match parent.nodes() {
            Some(n) => n,
            None => return DisplayType::None,
        };
        for i in nodes {
            if let NodeKind::Declaration(d) = &i.kind {
                if d.prop != "display" {
                    continue;
                }
                if d.value.contains("flex") {
                    return DisplayType::Flex;
                }
                if d.value.contains("grid") {
                    return DisplayType::Grid;
                }
            }
        }
        DisplayType::None
    }

    /// JS: `withHackValue(decl)`.
    /// ```js
    /// withHackValue(decl) {
    ///   return decl.prop === '-webkit-background-clip' && decl.value === 'text'
    /// }
    /// ```
    pub fn with_hack_value(&self, decl: &Node) -> bool {
        match &decl.kind {
            NodeKind::Declaration(d) => {
                d.prop == "-webkit-background-clip" && d.value == "text"
            }
            _ => false,
        }
    }

    /// JS: `reduceSpaces(decl)`.
    /// ```js
    /// reduceSpaces(decl) {
    ///   let stop = false
    ///   this.prefixes.group(decl).up(() => {
    ///     stop = true
    ///     return true
    ///   })
    ///   if (stop) return
    ///
    ///   let parts = decl.raw('before').split('\n')
    ///   let prevMin = parts[parts.length - 1].length
    ///   let diff = false
    ///
    ///   this.prefixes.group(decl).down(other => {
    ///     parts = other.raw('before').split('\n')
    ///     let last = parts.length - 1
    ///     if (parts[last].length > prevMin) {
    ///       if (diff === false) diff = parts[last].length - prevMin
    ///       parts[last] = parts[last].slice(0, -diff)
    ///       other.raws.before = parts.join('\n')
    ///     }
    ///   })
    /// }
    /// ```
    ///
    /// Mutation note: JS mutates `other.raws.before` from inside the
    /// `.down(callback)` walk. The Rust `GroupView::down` callback gets
    /// `&Node` (immutable), so we collect the target paths + new
    /// `before` strings during the walk, then mutate after. Output bytes
    /// are identical — every iteration's `prevMin` calculation reads
    /// from the un-modified `before` (JS's `decl.raw('before')` and
    /// `other.raw('before')` are pure reads of the `raws.before` slot,
    /// not derived mid-loop), so deferring the writeback doesn't change
    /// the comparison.
    ///
    /// JS-vs-Rust subtlety in `diff`: JS `diff` is `false | number`. The
    /// callback only initialises `diff` on the first hit (`if diff ===
    /// false`), so subsequent hits reuse the SAME diff. If a later
    /// sibling has a tail-line LONGER than the first hit, JS still
    /// strips only `diff` chars from it, not however many it would need
    /// to match `prevMin`. Mirror this exactly.
    pub fn reduce_spaces(&self, root: &mut Node, path: &[usize]) {
        // JS: `up()` callback returns `true` on first call → up returns
        // truthy → `stop = true`. So `stop` is true iff there's at least
        // one prefixed sibling above.
        let group_up = match self.prefixes.group(root, path) {
            Some(g) => g,
            None => return,
        };
        let mut stop = false;
        group_up.up(root, |_| {
            stop = true;
            true
        });
        if stop {
            return;
        }

        // Snapshot the decl's tail-line length.
        let here_before = match node_at_path(root, path) {
            Some(n) => n.raws.before.clone().unwrap_or_default(),
            None => return,
        };
        let prev_min = here_before
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .chars()
            .count();

        // Re-grab a group view for the down walk (the borrow above
        // released; cheap to rebuild).
        let group_down = match self.prefixes.group(root, path) {
            Some(g) => g,
            None => return,
        };

        // Collect targets — JS down() yields siblings in forward order.
        // The callback closure here mirrors JS's diff/parts logic and
        // computes the new `before` for each sibling that needs one.
        let mut diff: Option<usize> = None;
        // (sibling_path_relative_to_walk, new_before).
        let mut updates: Vec<(Vec<usize>, String)> = Vec::new();
        // `down` doesn't expose the path, so we re-derive: forward
        // siblings live at `path[..-1] + (path[-1] + n)` where n
        // increments on each iteration.
        let mut sib_offset: usize = 0;
        group_down.down(root, |other| {
            sib_offset += 1;
            let parts: Vec<String> = other
                .raws
                .before
                .clone()
                .unwrap_or_default()
                .split('\n')
                .map(String::from)
                .collect();
            let last_idx = parts.len().saturating_sub(1);
            let last_len = parts.get(last_idx).map(|s| s.chars().count()).unwrap_or(0);
            if last_len > prev_min {
                let d = match diff {
                    Some(d) => d,
                    None => {
                        let new_d = last_len - prev_min;
                        diff = Some(new_d);
                        new_d
                    }
                };
                let mut new_parts = parts;
                if let Some(last) = new_parts.last_mut() {
                    // JS: `parts[last].slice(0, -diff)` — drop the last `diff` CHARS.
                    let take = last.chars().count().saturating_sub(d);
                    let truncated: String = last.chars().take(take).collect();
                    *last = truncated;
                }
                let new_before = new_parts.join("\n");
                let mut sib_path = path.to_vec();
                if let Some(p) = sib_path.last_mut() {
                    *p += sib_offset;
                }
                updates.push((sib_path, new_before));
            }
            false
        });

        for (sib_path, new_before) in updates {
            if let Some(node) = node_at_path_mut(root, &sib_path) {
                node.raws.before = Some(new_before);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefixes::Prefixes;
    use crate::test_support::afm_browsers;
    use postcss_core::parse;

    fn empty_prefixes() -> Prefixes {
        Prefixes::with_empty()
    }

    fn afm_prefixes() -> Prefixes {
        Prefixes::new(afm_browsers(), Default::default())
    }

    #[test]
    fn disabled_returns_false_on_root_with_no_directives() {
        let mut r = parse("a { color: red; }").unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        // root path
        assert!(!proc.disabled(&mut r.root, &[], &mut warnings));
    }

    #[test]
    fn disabled_caches_answer_on_node() {
        let mut r = parse("a { color: red; }").unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        // First call writes cache.
        let _ = proc.disabled(&mut r.root, &[], &mut warnings);
        let here = node_at_path(&r.root, &[]).unwrap();
        assert_eq!(here.attrs.get_bool(ATTR_DISABLED), Some(false));
    }

    #[test]
    fn disabled_ignore_next_comment_marks_self_disabled() {
        // `/* autoprefixer: ignore next */` comment immediately before a
        // rule disables that rule and ONLY that rule.
        let mut r = parse(
            "/* autoprefixer: ignore next */\na { color: red; }\nb { color: blue; }",
        )
        .unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        // path [1] = the `a` rule (after the comment).
        assert!(proc.disabled(&mut r.root, &[1], &mut warnings));
        let here = node_at_path(&r.root, &[1]).unwrap();
        assert_eq!(here.attrs.get_bool(ATTR_SELF_DISABLED), Some(true));
        // path [2] = `b` rule — NOT disabled (only `a` was).
        assert!(!proc.disabled(&mut r.root, &[2], &mut warnings));
    }

    #[test]
    fn disabled_off_block_disables_descendants() {
        // `/* autoprefixer: off */` inside a container disables every
        // descendant.
        let mut r = parse(
            "a {\n  /* autoprefixer: off */\n  color: red;\n}",
        )
        .unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        // path [0] = the `a` rule. The off comment inside makes it disabled.
        assert!(proc.disabled(&mut r.root, &[0], &mut warnings));
        // path [0, 1] = `color: red;` — its parent is disabled (not
        // self-disabled), so it inherits → disabled.
        assert!(proc.disabled(&mut r.root, &[0, 1], &mut warnings));
    }

    #[test]
    fn disabled_second_control_comment_emits_warning() {
        let mut r = parse(
            "a {\n  /* autoprefixer: off */\n  /* autoprefixer: on */\n  color: red;\n}",
        )
        .unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        let _ = proc.disabled(&mut r.root, &[0], &mut warnings);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Second Autoprefixer control comment"));
    }

    #[test]
    fn disabled_decl_disables_grid_props_when_grid_off() {
        // No grid directive anywhere, options.grid = None → grid is off.
        // A `grid-template` decl should be disabled.
        let mut r = parse("a { grid-template-columns: 1fr; }").unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        assert!(proc.disabled_decl(&mut r.root, &[0, 0], &mut warnings));
    }

    #[test]
    fn disabled_decl_passes_through_non_grid_when_grid_off() {
        let mut r = parse("a { color: red; }").unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        // grid is off, prop doesn't include grid → falls through to
        // generic disabled() which returns false.
        assert!(!proc.disabled_decl(&mut r.root, &[0, 0], &mut warnings));
    }

    #[test]
    fn disabled_value_disables_display_grid_when_grid_off() {
        let mut r = parse("a { display: grid; }").unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        assert!(proc.disabled_value(&mut r.root, &[0, 0], &mut warnings));
    }

    #[test]
    fn disabled_value_disables_content_decl_unconditionally() {
        let mut r = parse("a { content: 'x'; }").unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        assert!(proc.disabled_value(&mut r.root, &[0, 0], &mut warnings));
    }

    #[test]
    fn grid_status_default_off_at_root() {
        let mut r = parse("a { color: red; }").unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        assert_eq!(
            proc.grid_status(&mut r.root, &[], &mut warnings),
            GridStatus::Off
        );
    }

    #[test]
    fn grid_status_on_via_options_grid() {
        let mut r = parse("a { color: red; }").unwrap();
        let mut p = empty_prefixes();
        p.options.grid = Some("true".into());
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        assert_eq!(
            proc.grid_status(&mut r.root, &[], &mut warnings),
            GridStatus::On
        );
    }

    #[test]
    fn grid_status_autoplace_via_options_grid() {
        let mut r = parse("a { color: red; }").unwrap();
        let mut p = empty_prefixes();
        p.options.grid = Some("autoplace".into());
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        assert_eq!(
            proc.grid_status(&mut r.root, &[], &mut warnings),
            GridStatus::Autoplace
        );
    }

    #[test]
    fn grid_status_on_comment_enables_block() {
        let mut r = parse(
            "a {\n  /* autoprefixer grid: on */\n  display: grid;\n}",
        )
        .unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        assert_eq!(
            proc.grid_status(&mut r.root, &[0], &mut warnings),
            GridStatus::On
        );
    }

    #[test]
    fn grid_status_autoplace_comment_enables_autoplace() {
        let mut r = parse(
            "a {\n  /* autoprefixer grid: autoplace */\n  display: grid;\n}",
        )
        .unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        assert_eq!(
            proc.grid_status(&mut r.root, &[0], &mut warnings),
            GridStatus::Autoplace
        );
    }

    #[test]
    fn grid_status_supports_grid_auto_forces_off() {
        // `@supports (grid auto) { ... }` overrides any inner status.
        // The supports-special branch sets value=false even if a child
        // comment said `grid: on`.
        let mut r = parse(
            "@supports (grid auto) { a { display: grid; } }",
        )
        .unwrap();
        let p = empty_prefixes();
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        // path [0] = @supports atrule.
        assert_eq!(
            proc.grid_status(&mut r.root, &[0], &mut warnings),
            GridStatus::Off
        );
    }

    #[test]
    fn grid_status_caches_answer_as_attr() {
        let mut r = parse("a { color: red; }").unwrap();
        let mut p = empty_prefixes();
        p.options.grid = Some("autoplace".into());
        let proc = Processor::new(&p);
        let mut warnings = Vec::new();
        let _ = proc.grid_status(&mut r.root, &[], &mut warnings);
        let here = node_at_path(&r.root, &[]).unwrap();
        match here.attrs.get(ATTR_GRID_STATUS) {
            Some(AttrValue::String(s)) => assert_eq!(s, "autoplace"),
            other => panic!("expected String('autoplace'), got {:?}", other),
        }
    }

    #[test]
    fn display_type_returns_flex_when_sibling_display_is_flex() {
        let r = parse("a { display: flex; align-self: center; }").unwrap();
        let p = afm_prefixes();
        let proc = Processor::new(&p);
        // path [0, 1] = `align-self`, sibling 0 = `display: flex`.
        assert_eq!(proc.display_type(&r.root, &[0, 1]), DisplayType::Flex);
    }

    #[test]
    fn display_type_returns_grid_when_sibling_display_is_grid() {
        let r = parse("a { display: grid; grid-row: 1; }").unwrap();
        let p = afm_prefixes();
        let proc = Processor::new(&p);
        assert_eq!(proc.display_type(&r.root, &[0, 1]), DisplayType::Grid);
    }

    #[test]
    fn display_type_returns_none_when_no_display_decl() {
        let r = parse("a { color: red; }").unwrap();
        let p = afm_prefixes();
        let proc = Processor::new(&p);
        assert_eq!(proc.display_type(&r.root, &[0, 0]), DisplayType::None);
    }

    #[test]
    fn with_hack_value_matches_webkit_background_clip_text() {
        let r = parse("a { -webkit-background-clip: text; }").unwrap();
        let p = afm_prefixes();
        let proc = Processor::new(&p);
        let decl = node_at_path(&r.root, &[0, 0]).unwrap();
        assert!(proc.with_hack_value(decl));
    }

    #[test]
    fn with_hack_value_does_not_match_other_props() {
        let r = parse("a { background-clip: text; }").unwrap();
        let p = afm_prefixes();
        let proc = Processor::new(&p);
        let decl = node_at_path(&r.root, &[0, 0]).unwrap();
        assert!(!proc.with_hack_value(decl));
    }

    #[test]
    fn reduce_spaces_does_nothing_when_no_prefixed_siblings() {
        let mut r = parse("a {\n    display: flex;\n}").unwrap();
        let p = afm_prefixes();
        let proc = Processor::new(&p);
        let before_initial = node_at_path(&r.root, &[0, 0])
            .unwrap()
            .raws
            .before
            .clone();
        // No prefixed siblings → `up()` finds none → stop=false, but
        // `down()` also has no targets, so a no-op.
        proc.reduce_spaces(&mut r.root, &[0, 0]);
        let before_after = node_at_path(&r.root, &[0, 0])
            .unwrap()
            .raws
            .before
            .clone();
        assert_eq!(before_initial, before_after);
    }

    #[test]
    fn reduce_spaces_early_returns_when_prefixed_sibling_above_exists() {
        // The unprefixed `display` has a `-webkit-display` sibling above
        // it. `up()` finds at least one, sets stop=true, and reduce_spaces
        // returns without touching `down()` at all. Pin: the down-walk
        // siblings (none here, but conceptually) MUST NOT be modified.
        let mut r = parse(
            "a {\n  -webkit-display: flex;\n  display: flex;\n}",
        )
        .unwrap();
        let p = afm_prefixes();
        let proc = Processor::new(&p);
        let webkit_before = node_at_path(&r.root, &[0, 0])
            .unwrap()
            .raws
            .before
            .clone();
        // path [0, 1] = unprefixed display.
        proc.reduce_spaces(&mut r.root, &[0, 1]);
        // -webkit-display untouched.
        let webkit_after = node_at_path(&r.root, &[0, 0])
            .unwrap()
            .raws
            .before
            .clone();
        assert_eq!(webkit_before, webkit_after);
    }
}
