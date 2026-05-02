//! Port of `postcss/lib/stringifier.js`.
//!
//! Line-numbered references in this file point at upstream `stringifier.js`.
//! Walks the AST and reconstructs the original byte sequence using each
//! node's `raws`. When a node's `raws.before` is `None` (e.g. inserted
//! by a plugin), we derive a default by scanning the tree for a sample
//! value — the same algorithm upstream uses for `rawCache`.

use crate::node::{Node, NodeKind};
use crate::root::Root;

/// Default raws — line 3 upstream. Used as the final fallback when no
/// sample is found anywhere in the tree.
fn default_raw(name: &str) -> &'static str {
    match name {
        "after" | "beforeClose" | "beforeComment" | "beforeDecl" | "beforeRule" => "\n",
        "beforeOpen" => " ",
        "colon" => ": ",
        "commentLeft" | "commentRight" => " ",
        "indent" => "    ",
        _ => "",
    }
}

pub struct Stringifier {
    out: String,
    /// `rawCache` upstream — populated lazily on top-level stringify of a
    /// Root and consulted whenever a child node has `raws.before == None`.
    /// `Some(s)` means a sample was found and processed; `None` (with
    /// `cache_populated == true`) means nothing in the tree had a sample
    /// for this slot — caller falls back to `default_raw`.
    cache_before_rule: Option<String>,
    cache_before_decl: Option<String>,
    cache_before_comment: Option<String>,
    /// `rawBeforeClose` upstream — used when a non-Root container's
    /// `raws.after` is `None`. Scans for the first container with
    /// `nodes.length > 0` and a defined `raws.after`, takes that
    /// value with trailing non-newline stripped then non-whitespace
    /// stripped.
    cache_before_close: Option<String>,
    /// `rawBeforeOpen` upstream — line 237. Sample for the
    /// `between` slot on Rule/AtRule nodes whose own
    /// `raws.between == None`. Walks the tree for the first non-decl
    /// node with a defined `raws.between`; no transform applied.
    /// Falls back to `DEFAULT_RAW["beforeOpen"] = " "`.
    cache_before_open: Option<String>,
    /// `rawSemicolon` upstream — line 304. Sample for the
    /// `raws.semicolon` flag on a container whose `raws.semicolon ==
    /// None`. Walks for the first container with non-empty body whose
    /// last child is a Declaration AND has a defined `raws.semicolon`.
    /// Falls back to `false` (the upstream `DEFAULT_RAW["semicolon"]`).
    cache_semicolon: Option<bool>,
    /// `rawColon` upstream — line 265. Sample for the `between` slot
    /// on a Declaration node whose own `raws.between == None`. Walks
    /// for the first decl with a defined `raws.between`, then strips
    /// any character that is neither whitespace nor `:`. Falls back to
    /// `DEFAULT_RAW["colon"] = ": "`.
    cache_colon: Option<String>,
    cache_populated: bool,
    /// Plugin-written `root.rawCache` overrides — populated from the
    /// Root passed into `stringify_root`. Consulted at upstream's
    /// **step-3 priority** (BEFORE the tree-scan fallback).
    /// See `postcss/lib/stringifier.js::raw()` line 158.
    ///
    /// Today's pipeline (transformCss / sort) filters out
    /// `cssnano-util-raw-cache` so this map stays empty — the
    /// stringifier falls through to step 4. The mechanism still has
    /// to be wired correctly because *any* future plugin that writes
    /// `root.rawCache` (or a custom toolchain that bundles
    /// cssnano-util-raw-cache) must produce identical bytes to JS.
    raw_cache_overrides: indexmap::IndexMap<String, String>,
}

impl Stringifier {
    pub fn new() -> Self {
        Stringifier {
            out: String::new(),
            cache_before_rule: None,
            cache_before_decl: None,
            cache_before_comment: None,
            cache_before_close: None,
            cache_before_open: None,
            cache_semicolon: None,
            cache_colon: None,
            cache_populated: false,
            raw_cache_overrides: indexmap::IndexMap::new(),
        }
    }

    pub fn finish(self) -> String { self.out }

    /// Top-level — stringifies a Root.
    ///
    /// Priority chain for the trailing `raws.after` on the Root itself:
    ///   1. `root.root.raws.after` — verbatim if set.
    ///   2. `root.rawCache.after` — plugin override (cssnano-util-raw-cache
    ///      writes this) per upstream stringifier.js:158.
    ///   3. (no tree-scan / no DEFAULT_RAW for trailing root after — emit
    ///      nothing, matches upstream's `if (after) builder(after)`.)
    pub fn stringify_root(&mut self, root: &Root) {
        self.raw_cache_overrides = root.raw_cache.clone();
        self.populate_cache(&root.root);
        self.body(&root.root);
        match &root.root.raws.after {
            Some(after) => self.out.push_str(after),
            None => {
                let override_after = self.raw_cache_override("after").map(str::to_string);
                if let Some(v) = override_after {
                    self.out.push_str(&v);
                }
            }
        }
    }

    fn populate_cache(&mut self, root_node: &Node) {
        if self.cache_populated { return; }
        self.cache_before_rule = scan_before_rule(root_node);
        self.cache_before_decl = scan_before_decl(root_node);
        self.cache_before_comment = scan_before_comment(root_node);
        self.cache_before_close = scan_before_close(root_node);
        self.cache_before_open = scan_before_open(root_node);
        self.cache_semicolon = scan_semicolon(root_node);
        self.cache_colon = scan_colon(root_node);
        self.cache_populated = true;
    }

    /// Mirrors upstream `raw(node, own, detect)` priority for the rawCache
    /// step (line 158): if `root.rawCache[detect] !== undefined`, return
    /// that VERBATIM (no fallback to tree-scan). Returns `None` when no
    /// override is set, leaving the caller to use its tree-scan cache +
    /// `DEFAULT_RAW` fallback chain.
    ///
    /// This is consulted at step 3 of upstream's priority chain. Steps
    /// 1-2 (`node.raws[own]` + before-on-root-first / document-parent)
    /// are owned by individual call sites in this stringifier; step 4-5
    /// (tree-scan + DEFAULT_RAW) are owned by the existing `cache_*`
    /// fields and the `default_raws_*` helpers below.
    fn raw_cache_override(&self, detect: &str) -> Option<&str> {
        self.raw_cache_overrides.get(detect).map(|s| s.as_str())
    }

    /// Stringify a single node into the buffer using upstream's
    /// "standalone" semantics: dispatch on kind with `semicolon=false`
    /// (matches `node.toString()` upstream, which calls `stringify(node)`
    /// on a fresh stringifier with no semicolon flag — see
    /// `postcss/lib/stringifier.js::stringify`).
    ///
    /// Notably this does NOT emit the node's `raws.before`. That's a
    /// parent-context concern owned by `body()`. Plugins like
    /// `extract-stylesheets` rely on this to emit each top-level child
    /// as a self-contained sheet.
    pub fn stringify_node(&mut self, node: &Node) {
        self.stringify(node, false);
    }

    /// `stringify(node, semicolon)` — line 337.
    fn stringify(&mut self, node: &Node, semicolon: bool) {
        match &node.kind {
            NodeKind::Root(_) => self.stringify_node_root(node),
            NodeKind::AtRule(_) => self.atrule(node, semicolon),
            NodeKind::Rule(_) => self.rule(node),
            NodeKind::Declaration(_) => self.decl(node, semicolon),
            NodeKind::Comment(_) => self.comment(node),
        }
    }

    fn stringify_node_root(&mut self, node: &Node) {
        self.body(node);
        if let Some(after) = &node.raws.after { self.out.push_str(after); }
    }

    /// `atrule(node, semicolon)` — line 27.
    fn atrule(&mut self, node: &Node, semicolon: bool) {
        let at = match &node.kind { NodeKind::AtRule(a) => a, _ => unreachable!() };
        let mut name = String::from("@");
        name.push_str(&at.name);

        let params = if !at.params.is_empty() { self.raw_value_str(node, &at.params, node.raws.params.as_ref()) } else { String::new() };
        if let Some(after_name) = &node.raws.after_name { name.push_str(after_name); }
        else if !params.is_empty() { name.push(' '); }

        if at.has_block {
            self.block(node, &(name + &params));
        } else {
            let between = node.raws.between.clone().unwrap_or_default();
            self.out.push_str(&name);
            self.out.push_str(&params);
            self.out.push_str(&between);
            if semicolon { self.out.push(';'); }
        }
    }

    /// `rule(node)` — line 330.
    fn rule(&mut self, node: &Node) {
        let r = match &node.kind { NodeKind::Rule(r) => r, _ => unreachable!() };
        let selector = self.raw_value_str(node, &r.selector, node.raws.selector.as_ref());
        self.block(node, &selector);
        if let Some(own_semi) = &node.raws.own_semicolon { self.out.push_str(own_semi); }
    }

    /// `decl(node, semicolon)` — line 112.
    fn decl(&mut self, node: &Node, semicolon: bool) {
        let d = match &node.kind { NodeKind::Declaration(d) => d, _ => unreachable!() };
        // Priority: node.raws.between (step 1) → root.rawCache.colon
        // (step 3) → tree-scan colon (step 4) → DEFAULT_RAW.colon (step 5).
        let between = match &node.raws.between {
            Some(b) => b.clone(),
            None => {
                if let Some(v) = self.raw_cache_override("colon") {
                    v.to_string()
                } else {
                    self.cache_colon.clone().unwrap_or_else(|| default_raw("colon").to_string())
                }
            }
        };
        let value = self.raw_value_str(node, &d.value, node.raws.value.as_ref());
        let mut s = String::new();
        s.push_str(&d.prop);
        s.push_str(&between);
        s.push_str(&value);
        if d.important {
            s.push_str(node.raws.important.as_deref().unwrap_or(" !important"));
        }
        if semicolon { s.push(';'); }
        self.out.push_str(&s);
    }

    /// `comment(node)` — line 106. Priority chain on `left` / `right`:
    /// node.raws.{left,right} (step 1) → root.rawCache.{commentLeft,
    /// commentRight} (step 3) → DEFAULT_RAW.{commentLeft, commentRight}
    /// = `" "` (step 5). No tree-scan fallback for these — upstream
    /// hits `DEFAULT_RAW` directly when both are unset.
    fn comment(&mut self, node: &Node) {
        let c = match &node.kind { NodeKind::Comment(c) => c, _ => unreachable!() };
        let left = node.raws.left.as_deref().map(str::to_string).unwrap_or_else(|| {
            self.raw_cache_override("commentLeft")
                .map(str::to_string)
                .unwrap_or_else(|| default_raw("commentLeft").to_string())
        });
        let right = node.raws.right.as_deref().map(str::to_string).unwrap_or_else(|| {
            self.raw_cache_override("commentRight")
                .map(str::to_string)
                .unwrap_or_else(|| default_raw("commentRight").to_string())
        });
        self.out.push_str("/*");
        self.out.push_str(&left);
        self.out.push_str(&c.text);
        self.out.push_str(&right);
        self.out.push_str("*/");
    }

    /// `block(node, start)` — line 74.
    fn block(&mut self, node: &Node, start: &str) {
        // Mirrors upstream `between = this.raw(node, 'between', 'beforeOpen')`:
        // when `raws.between` is undefined, fall back to the rawCache
        // `beforeOpen` scan, then to `DEFAULT_RAW["beforeOpen"] = " "`.
        let between = match &node.raws.between {
            Some(b) => b.clone(),
            None => self.default_raws_between(),
        };
        self.out.push_str(start);
        self.out.push_str(&between);
        self.out.push('{');
        if let Some(children) = node.nodes() {
            if !children.is_empty() {
                self.body(node);
                // Mirrors upstream `after = this.raw(node, 'after')`:
                // when raws.after is undefined, scan via rawBeforeClose
                // (cached) and fall back to the DEFAULT_RAW table.
                match &node.raws.after {
                    Some(after) => self.out.push_str(after),
                    None => self.out.push_str(&self.default_raws_after()),
                }
            } else {
                // Empty body: upstream `after = this.raw(node, 'after', 'emptyBody')`.
                // Priority: node.raws.after (step 1) → rawCache.emptyBody
                // (step 3) → tree-scan rawEmptyBody (not implemented; would
                // need a fourth scanner) → DEFAULT_RAW.emptyBody (`""`).
                match &node.raws.after {
                    Some(after) => self.out.push_str(after),
                    None => {
                        let override_empty = self.raw_cache_override("emptyBody").map(str::to_string);
                        if let Some(v) = override_empty {
                            self.out.push_str(&v);
                        }
                        // else emit nothing (matches `DEFAULT_RAW.emptyBody = ""`).
                    }
                }
            }
        }
        self.out.push('}');
    }

    /// Default `raws.after` for a non-empty container body. Mirrors
    /// the `beforeClose` branch of upstream `beforeAfter`. Priority:
    /// `root.rawCache.beforeClose` (step 3) → tree-scan (step 4) →
    /// `DEFAULT_RAW.beforeClose = "\n"` (step 5).
    fn default_raws_after(&self) -> String {
        if let Some(v) = self.raw_cache_override("beforeClose") {
            return v.to_string();
        }
        self.cache_before_close
            .clone()
            .unwrap_or_else(|| default_raw("beforeClose").to_string())
    }

    /// Default `raws.between` for a Rule/AtRule with no `raws.between`
    /// set. Priority: `root.rawCache.beforeOpen` (step 3) → tree-scan
    /// (step 4) → `DEFAULT_RAW.beforeOpen = " "` (step 5).
    fn default_raws_between(&self) -> String {
        if let Some(v) = self.raw_cache_override("beforeOpen") {
            return v.to_string();
        }
        self.cache_before_open
            .clone()
            .unwrap_or_else(|| default_raw("beforeOpen").to_string())
    }

    /// `body(node)` — line 90.
    fn body(&mut self, node: &Node) {
        let children = match node.nodes() { Some(c) => c, None => return };
        // Find the last non-comment index.
        let mut last = if children.is_empty() { 0 } else { children.len() - 1 };
        while last > 0 {
            if !matches!(children[last].kind, NodeKind::Comment(_)) { break; }
            last -= 1;
        }
        let parent_is_root = matches!(node.kind, NodeKind::Root(_));
        // Mirrors upstream `semicolon = this.raw(node, 'semicolon')`:
        // when `raws.semicolon` is undefined, fall back to the rawCache
        // `semicolon` scan; then to `DEFAULT_RAW["semicolon"] = false`.
        let semicolon = match node.raws.semicolon {
            Some(s) => s,
            None => self.cache_semicolon.unwrap_or(false),
        };
        for (i, child) in children.iter().enumerate() {
            match &child.raws.before {
                Some(before) => self.out.push_str(before),
                None => {
                    // Upstream `raw(child, 'before')` — special case for
                    // first child of root → empty string. Otherwise
                    // dispatch by kind to the corresponding cache slot,
                    // falling back to `DEFAULT_RAW` when nothing was
                    // found in the tree.
                    let defaulted = if parent_is_root && i == 0 {
                        String::new()
                    } else {
                        self.default_raws_before(child)
                    };
                    self.out.push_str(&defaulted);
                }
            }
            self.stringify(child, last != i || semicolon);
        }
    }

    fn default_raws_before(&self, child: &Node) -> String {
        // Priority: rawCache override (step 3) → tree-scan (step 4) →
        // DEFAULT_RAW (step 5).
        let detect = match &child.kind {
            NodeKind::Declaration(_) => "beforeDecl",
            NodeKind::Comment(_) => "beforeComment",
            _ => "beforeRule",
        };
        if let Some(v) = self.raw_cache_override(detect) {
            return v.to_string();
        }
        match &child.kind {
            NodeKind::Declaration(_) => self
                .cache_before_decl
                .clone()
                .unwrap_or_else(|| default_raw("beforeDecl").to_string()),
            NodeKind::Comment(_) => self
                .cache_before_comment
                .clone()
                .unwrap_or_else(|| default_raw("beforeComment").to_string()),
            // Rule / AtRule / Root.
            _ => self
                .cache_before_rule
                .clone()
                .unwrap_or_else(|| default_raw("beforeRule").to_string()),
        }
    }

    /// `rawValue(node, prop)` — line 315.
    fn raw_value_str(&self, _node: &Node, value: &str, raw: Option<&crate::node::RawValue>) -> String {
        if let Some(rv) = raw {
            if rv.value == value { return rv.raw.clone(); }
        }
        value.to_string()
    }
}

impl Default for Stringifier {
    fn default() -> Self { Self::new() }
}

pub fn stringify(root: &Root) -> String {
    let mut s = Stringifier::new();
    s.stringify_root(root);
    s.finish()
}

/// Stringify a single AST [`Node`] standalone — used by plugins like
/// `extract-stylesheets` that emit per-child sheets. Mirrors
/// `node.toString()` upstream.
pub fn stringify_node(node: &Node) -> String {
    let mut s = Stringifier::new();
    s.stringify_node(node);
    s.finish()
}

// --------------------------------------------------------------------------
// rawBeforeXxx scanners — port of stringifier.js lines 182-235.
//
// Each scanner walks the entire tree (depth-first) looking for the first
// "useful" sample of a sibling's `raws.before`, normalizes it, and returns
// it. Callers (via `body()`) use the result whenever a child has
// `raws.before == None`, falling back to `default_raw` if no sample was
// found.
// --------------------------------------------------------------------------

/// `rawBeforeRule(root)` — line 248. Find first container (with `nodes`)
/// where it's not the first child of root, and use its `raws.before`.
/// `value.replace(/[^\n]+$/, '')` strips the trailing non-newline run
/// (i.e. keeps only the leading newlines), then `value.replace(/\S/g, '')`
/// strips any remaining non-whitespace characters — the result is the
/// "indent prefix" (newlines + spaces) that should precede each rule.
fn scan_before_rule(root_node: &Node) -> Option<String> {
    let mut found: Option<String> = None;
    walk_for_rule_sample(root_node, root_node, &mut found);
    found.map(|v| {
        let trimmed = strip_trailing_non_newline(&v);
        strip_non_whitespace(&trimmed)
    })
}

fn walk_for_rule_sample(root_node: &Node, n: &Node, out: &mut Option<String>) {
    if out.is_some() { return; }
    if let Some(children) = n.nodes() {
        for child in children {
            if out.is_some() { return; }
            // Container? (has its own `nodes`)
            let is_container = child.nodes().is_some();
            if is_container {
                let parent_is_root = std::ptr::eq(n, root_node);
                let is_first_of_root = parent_is_root
                    && children.first().map(|c| std::ptr::eq(c, child)).unwrap_or(false);
                if !(parent_is_root && is_first_of_root) {
                    if let Some(before) = &child.raws.before {
                        *out = Some(before.clone());
                        return;
                    }
                }
            }
            walk_for_rule_sample(root_node, child, out);
        }
    }
}

/// `rawBeforeDecl(root, node)` — line 218. Find first decl with a
/// defined `raws.before`. Process: strip trailing non-newline chars,
/// then non-whitespace chars. If none found, fall back to
/// `rawBeforeRule`.
fn scan_before_decl(root_node: &Node) -> Option<String> {
    let mut found: Option<String> = None;
    walk_for_decl_sample(root_node, &mut found);
    found
        .map(|v| {
            let trimmed = strip_trailing_non_newline(&v);
            strip_non_whitespace(&trimmed)
        })
        .or_else(|| scan_before_rule(root_node))
}

fn walk_for_decl_sample(n: &Node, out: &mut Option<String>) {
    if out.is_some() { return; }
    if let Some(children) = n.nodes() {
        for child in children {
            if out.is_some() { return; }
            if matches!(child.kind, NodeKind::Declaration(_)) {
                if let Some(before) = &child.raws.before {
                    *out = Some(before.clone());
                    return;
                }
            }
            walk_for_decl_sample(child, out);
        }
    }
}

/// `rawBeforeClose(root)` — line 182. Find the first container with
/// `nodes.length > 0` and a defined `raws.after`. Take its `raws.after`,
/// strip trailing non-newline run, then strip non-whitespace chars.
fn scan_before_close(root_node: &Node) -> Option<String> {
    let mut found: Option<String> = None;
    walk_for_close_sample(root_node, &mut found);
    found.map(|v| {
        let trimmed = strip_trailing_non_newline(&v);
        strip_non_whitespace(&trimmed)
    })
}

fn walk_for_close_sample(n: &Node, out: &mut Option<String>) {
    if out.is_some() { return; }
    if let Some(children) = n.nodes() {
        for child in children {
            if out.is_some() { return; }
            // Container with non-empty body and defined raws.after?
            if let Some(grandkids) = child.nodes() {
                if !grandkids.is_empty() {
                    if let Some(after) = &child.raws.after {
                        *out = Some(after.clone());
                        return;
                    }
                }
            }
            walk_for_close_sample(child, out);
        }
    }
}

/// `rawBeforeComment(root, node)` — line 199. Find first comment with a
/// defined `raws.before`. Falls back to `rawBeforeDecl`.
fn scan_before_comment(root_node: &Node) -> Option<String> {
    let mut found: Option<String> = None;
    walk_for_comment_sample(root_node, &mut found);
    found
        .map(|v| {
            let trimmed = strip_trailing_non_newline(&v);
            strip_non_whitespace(&trimmed)
        })
        .or_else(|| scan_before_decl(root_node))
}

fn walk_for_comment_sample(n: &Node, out: &mut Option<String>) {
    if out.is_some() { return; }
    if let Some(children) = n.nodes() {
        for child in children {
            if out.is_some() { return; }
            if matches!(child.kind, NodeKind::Comment(_)) {
                if let Some(before) = &child.raws.before {
                    *out = Some(before.clone());
                    return;
                }
            }
            walk_for_comment_sample(child, out);
        }
    }
}

/// JS `value.replace(/[^\n]+$/, '')` — strip the trailing run of
/// non-newline characters, ONLY if the value contains at least one
/// newline. Upstream guards this with `if (value.includes('\n'))`,
/// so values that are just spaces (no newline) pass through unchanged.
fn strip_trailing_non_newline(s: &str) -> String {
    if !s.contains('\n') {
        return s.to_string();
    }
    if let Some(last_nl) = s.rfind('\n') {
        s[..=last_nl].to_string()
    } else {
        s.to_string()
    }
}

/// `rawBeforeOpen(root)` — line 237. Find the first non-decl node with
/// a defined `raws.between` and return it verbatim (no normalization).
/// Used as the default for `Rule.raws.between` / `AtRule.raws.between`
/// when the node itself doesn't have one.
fn scan_before_open(root_node: &Node) -> Option<String> {
    let mut found: Option<String> = None;
    walk_for_open_sample(root_node, &mut found);
    found
}

fn walk_for_open_sample(n: &Node, out: &mut Option<String>) {
    if out.is_some() { return; }
    if let Some(children) = n.nodes() {
        for child in children {
            if out.is_some() { return; }
            // Upstream's `if (i.type !== 'decl')` check.
            if !matches!(child.kind, NodeKind::Declaration(_)) {
                if let Some(between) = &child.raws.between {
                    *out = Some(between.clone());
                    return;
                }
            }
            walk_for_open_sample(child, out);
        }
    }
}

/// `rawColon(root)` — line 265. Find first decl with defined
/// `raws.between`, strip any character not in `[\s:]`. The result is
/// the indent-stripped `:` separator used by fresh decls that were
/// built without their own `raws.between`.
fn scan_colon(root_node: &Node) -> Option<String> {
    let mut found: Option<String> = None;
    walk_for_colon_sample(root_node, &mut found);
    found.map(|v| {
        // JS `value.replace(/[^\s:]/g, '')` — JS `\s` set, not Rust's.
        v.chars()
            .filter(|c| is_js_regex_whitespace(*c) || *c == ':')
            .collect()
    })
}

fn walk_for_colon_sample(n: &Node, out: &mut Option<String>) {
    if out.is_some() { return; }
    if let Some(children) = n.nodes() {
        for child in children {
            if out.is_some() { return; }
            if let NodeKind::Declaration(_) = &child.kind {
                if let Some(between) = &child.raws.between {
                    *out = Some(between.clone());
                    return;
                }
            }
            walk_for_colon_sample(child, out);
        }
    }
}

/// `rawSemicolon(root)` — line 304. Find the first container with a
/// non-empty body whose **last child is a Declaration** and has a
/// defined `raws.semicolon`. Returns the boolean.
fn scan_semicolon(root_node: &Node) -> Option<bool> {
    let mut found: Option<bool> = None;
    walk_for_semicolon_sample(root_node, &mut found);
    found
}

fn walk_for_semicolon_sample(n: &Node, out: &mut Option<bool>) {
    if out.is_some() { return; }
    if let Some(children) = n.nodes() {
        // Check this node's own qualification first — `i.last.type ===
        // 'decl' && i.raws.semicolon !== undefined`.
        if let Some(last) = children.last() {
            if matches!(last.kind, NodeKind::Declaration(_)) {
                if let Some(s) = n.raws.semicolon {
                    *out = Some(s);
                    return;
                }
            }
        }
        for child in children {
            if out.is_some() { return; }
            walk_for_semicolon_sample(child, out);
        }
    }
}

/// JS `value.replace(/\S/g, '')` — keep only chars matching JS `\s`.
///
/// ECMAScript `\s` is defined precisely as:
///   WhiteSpace      = { U+0009 TAB, U+000B VT, U+000C FF, U+0020 SP,
///                       U+00A0 NBSP, U+FEFF ZWNBSP }
///                     ∪ Unicode `Space_Separator` general category
///   LineTerminator  = { U+000A LF, U+000D CR, U+2028 LS, U+2029 PS }
///
/// **This is NOT the same as Rust's `char::is_whitespace`**, which
/// returns the Unicode `White_Space` property. The two differ in:
///   - U+0085 (NEL): IN Rust `is_whitespace`, NOT in JS `\s`.
///   - U+FEFF: IN JS `\s`, NOT in Rust `is_whitespace`.
///
/// We enumerate the JS spec set directly. Don't replace this with
/// `is_whitespace` "for clarity" — a CSS input containing U+0085 in
/// a raws-before position would diverge byte-for-byte.
pub(crate) fn is_js_regex_whitespace(c: char) -> bool {
    match c {
        '\u{0009}'           // TAB
        | '\u{000A}'         // LF
        | '\u{000B}'         // VT
        | '\u{000C}'         // FF
        | '\u{000D}'         // CR
        | '\u{0020}'         // SP
        | '\u{00A0}'         // NBSP
        | '\u{2028}'         // LS
        | '\u{2029}'         // PS
        | '\u{FEFF}'         // ZWNBSP
            => true,
        // Unicode `Space_Separator` (Zs) — exhaustive list as of Unicode 16.
        // (NBSP U+00A0 already covered above.)
        '\u{1680}'
        | '\u{2000}'..='\u{200A}'
        | '\u{202F}'
        | '\u{205F}'
        | '\u{3000}'
            => true,
        _ => false,
    }
}

fn strip_non_whitespace(s: &str) -> String {
    s.chars().filter(|c| is_js_regex_whitespace(*c)).collect()
}

#[cfg(test)]
mod js_whitespace_tests {
    use super::*;

    /// Lock the JS `\s` ↔ Rust `White_Space` divergence: U+0085 is in
    /// Rust's `White_Space` but NOT in JS `\s`. JS treats it as
    /// non-whitespace; we must too.
    #[test]
    fn nel_u0085_is_not_js_whitespace() {
        assert!(!is_js_regex_whitespace('\u{0085}'),
            "U+0085 (NEL) is in Unicode White_Space but NOT in JS \\s");
        // strip_non_whitespace keeps only JS-whitespace chars; U+0085
        // must be stripped (treated as non-whitespace).
        assert_eq!(strip_non_whitespace("\u{0085}"), "");
        assert_eq!(strip_non_whitespace("a\u{0085}b"), "");
    }

    /// Lock the inverse: U+FEFF (ZWNBSP/BOM) IS JS `\s` but NOT in
    /// Rust's `White_Space`. We must keep it.
    #[test]
    fn zwnbsp_ufeff_is_js_whitespace() {
        assert!(is_js_regex_whitespace('\u{FEFF}'));
        assert_eq!(strip_non_whitespace("\u{FEFF}"), "\u{FEFF}");
    }

    /// Sanity: ASCII whitespace + LineTerminators are kept.
    #[test]
    fn ascii_whitespace_kept() {
        for c in ['\t', '\n', '\u{000B}', '\u{000C}', '\r', ' '] {
            assert!(is_js_regex_whitespace(c), "char {:?} should be JS whitespace", c);
        }
    }

    /// Space_Separator (Zs) characters are kept — covers Mongolian
    /// vowel separator (U+1680), various quad spaces (U+2000-200A),
    /// narrow no-break space (U+202F), medium math space (U+205F),
    /// ideographic space (U+3000).
    #[test]
    fn space_separator_kept() {
        for c in ['\u{1680}', '\u{2000}', '\u{200A}', '\u{202F}', '\u{205F}', '\u{3000}'] {
            assert!(is_js_regex_whitespace(c), "char U+{:04X} should be JS whitespace", c as u32);
        }
    }

    /// Non-whitespace characters get stripped.
    #[test]
    fn non_whitespace_stripped() {
        assert_eq!(strip_non_whitespace("a"), "");
        assert_eq!(strip_non_whitespace("abc"), "");
        assert_eq!(strip_non_whitespace("\n  abc \n"), "\n   \n");
    }
}
