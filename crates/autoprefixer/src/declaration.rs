//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/declaration.js`.
//!
//! `class Declaration extends Prefixer`. Hacks subclass this most often.

use indexmap::IndexMap;
use postcss_core::{insert_before_at_path, parent_some, AttrValue, Node, NodeKind};

use crate::browsers::Browsers;
use crate::prefixer::{clone_node, parent_prefix_cached_mut, ParentPrefix, PrefixerBase};
use crate::utils;

/// Per-decl bool memo for cascade decision.
pub const ATTR_CASCADE: &str = "_autoprefixerCascade";
/// Per-decl int memo for max prefix length (used by `calcBefore`).
pub const ATTR_MAX: &str = "_autoprefixerMax";

pub struct DeclarationBase {
    pub prefixer: PrefixerBase,
    pub cascade_option: bool,
}

impl DeclarationBase {
    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self {
            prefixer: PrefixerBase::new(name, prefixes, all_id),
            // JS default: `this.all.options.cascade !== false` — i.e.
            // cascade is on unless explicitly disabled.
            cascade_option: true,
        }
    }

    /// JS: `prefixed(prop, prefix) { return prefix + prop }`.
    pub fn prefixed(&self, prop: &str, prefix: &str) -> String {
        format!("{prefix}{prop}")
    }

    /// JS: `normalize(prop) { return prop }` (default; hacks override).
    pub fn normalize<'a>(&self, prop: &'a str) -> &'a str {
        prop
    }

    /// JS: `otherPrefixes(value, prefix)`.
    /// ```js
    /// for (let other of Browsers.prefixes()) {
    ///   if (other === prefix) continue
    ///   if (value.includes(other)) {
    ///     return value.replace(/var\([^)]+\)/, '').includes(other)
    ///   }
    /// }
    /// return false
    /// ```
    pub fn other_prefixes(&self, value: &str, prefix: &str) -> bool {
        static VAR_RE: once_cell::sync::Lazy<regex::Regex> =
            once_cell::sync::Lazy::new(|| {
                regex::Regex::new(r"var\([^)]+\)").unwrap()
            });
        for other in Browsers::prefixes() {
            if other == prefix {
                continue;
            }
            if value.contains(other.as_str()) {
                let stripped = VAR_RE.replace(value, "").into_owned();
                return stripped.contains(other.as_str());
            }
        }
        false
    }

    /// JS: `set(decl, prefix) { decl.prop = this.prefixed(decl.prop, prefix); return decl }`.
    pub fn set(&self, decl: &mut Node, prefix: &str) -> Option<()> {
        let new_prop = match &decl.kind {
            NodeKind::Declaration(d) => self.prefixed(&d.prop, prefix),
            _ => return None,
        };
        if let NodeKind::Declaration(ref mut d) = decl.kind {
            d.prop = new_prop;
        }
        Some(())
    }

    /// JS: `needCascade(decl)`.
    /// ```js
    /// if (!decl._autoprefixerCascade) {
    ///   decl._autoprefixerCascade = this.all.options.cascade !== false && decl.raw('before').includes('\n')
    /// }
    /// return decl._autoprefixerCascade
    /// ```
    pub fn need_cascade(&self, decl: &mut Node) -> bool {
        if let Some(b) = decl.attrs.get_bool(ATTR_CASCADE) {
            return b;
        }
        let answer = self.cascade_option && decl.raws.before.as_deref().map(|s| s.contains('\n')).unwrap_or(false);
        decl.attrs.set(ATTR_CASCADE, AttrValue::Bool(answer));
        answer
    }

    /// JS: `maxPrefixed(prefixes, decl)` — caches the longest prefix
    /// length on the decl. Lengths exclude the `removeNote` suffix.
    pub fn max_prefixed(&self, prefixes: &[String], decl: &mut Node) -> i64 {
        if let Some(i) = decl.attrs.get_int(ATTR_MAX) {
            return i;
        }
        let mut max: i64 = 0;
        for prefix in prefixes {
            let bare = utils::remove_note(prefix);
            if bare.len() as i64 > max {
                max = bare.len() as i64;
            }
        }
        decl.attrs.set(ATTR_MAX, AttrValue::Int(max));
        max
    }

    /// JS: `calcBefore(prefixes, decl, prefix)` — returns the new
    /// `raws.before` for a clone. Pads with spaces to align with the
    /// longest prefix.
    pub fn calc_before(
        &self,
        prefixes: &[String],
        decl: &mut Node,
        prefix: &str,
    ) -> String {
        let max = self.max_prefixed(prefixes, decl);
        let diff = max - utils::remove_note(prefix).len() as i64;

        let mut before = decl.raws.before.clone().unwrap_or_default();
        if diff > 0 {
            for _ in 0..diff {
                before.push(' ');
            }
        }
        before
    }

    /// JS: `restoreBefore(decl)`. Walks the decl's group (via `this.all
    /// .group(decl).up`) reading each `raw('before')` and chooses the
    /// shortest tail-line as the canonical indentation. We do not have
    /// `this.all.group` here yet — that lives in `prefixes.rs`. The
    /// processor will pass the group view in. For now this is a no-op
    /// placeholder; cascade-affecting tests live in `prefixes.rs` once
    /// that lands.
    pub fn restore_before(&self, _decl: &mut Node) {
        // TODO(phase-7): wire `Prefixes::group` from `prefixes.rs`.
    }

    /// JS: `insert(decl, prefix, prefixes)`.
    /// ```js
    /// let cloned = this.set(this.clone(decl), prefix)
    /// if (!cloned) return undefined
    /// let already = decl.parent.some(i => i.prop === cloned.prop && i.value === cloned.value)
    /// if (already) return undefined
    /// if (this.needCascade(decl)) cloned.raws.before = this.calcBefore(prefixes, decl, prefix)
    /// return decl.parent.insertBefore(decl, cloned)
    /// ```
    pub fn insert(
        &self,
        root: &mut Node,
        path: &[usize],
        prefix: &str,
        prefixes: &[String],
    ) -> Option<()> {
        let original = postcss_core::node_at_path(root, path)?;
        let mut cloned = clone_node(original);
        self.set(&mut cloned, prefix)?;

        let (cloned_prop, cloned_value) = match &cloned.kind {
            NodeKind::Declaration(d) => (d.prop.clone(), d.value.clone()),
            _ => return None,
        };

        let already = parent_some(root, path, |sibling| match &sibling.kind {
            NodeKind::Declaration(s) => {
                s.prop == cloned_prop && s.value == cloned_value
            }
            _ => false,
        });
        if already {
            return None;
        }

        // Cascade adjustment uses the original decl, not the clone.
        let need_cascade = {
            let here = postcss_core::node_at_path_mut(root, path)?;
            self.need_cascade(here)
        };
        if need_cascade {
            let here = postcss_core::node_at_path_mut(root, path)?;
            cloned.raws.before = Some(self.calc_before(prefixes, here, prefix));
        }

        insert_before_at_path(root, path, cloned);
        Some(())
    }

    /// JS: `isAlready(decl, prefixed)` — does a sibling-up search.
    /// `this.all.group(decl).up(...)` then `.down(...)`. Without the
    /// `Prefixes::group` view this is a placeholder; calls fall back
    /// to a shallow `parent_some` check on the same parent.
    pub fn is_already(&self, root: &Node, path: &[usize], prefixed: &str) -> bool {
        parent_some(root, path, |sibling| match &sibling.kind {
            NodeKind::Declaration(d) => d.prop == prefixed,
            _ => false,
        })
    }

    /// JS: `add(decl, prefix, prefixes, result)`.
    /// ```js
    /// let prefixed = this.prefixed(decl.prop, prefix)
    /// if (this.isAlready(decl, prefixed) || this.otherPrefixes(decl.value, prefix)) return undefined
    /// return this.insert(decl, prefix, prefixes, result)
    /// ```
    pub fn add(
        &self,
        root: &mut Node,
        path: &[usize],
        prefix: &str,
        prefixes: &[String],
    ) -> Option<()> {
        let (prop, value) = {
            let here = postcss_core::node_at_path(root, path)?;
            match &here.kind {
                NodeKind::Declaration(d) => (d.prop.clone(), d.value.clone()),
                _ => return None,
            }
        };
        let prefixed = self.prefixed(&prop, prefix);
        if self.is_already(root, path, &prefixed)
            || self.other_prefixes(&value, prefix)
        {
            return None;
        }
        self.insert(root, path, prefix, prefixes)
    }

    /// JS: `process(decl, result)` — overrides Prefixer.process with
    /// cascade handling.
    /// ```js
    /// process(decl, result) {
    ///   if (!this.needCascade(decl)) { super.process(decl, result); return }
    ///   let prefixes = super.process(decl, result)
    ///   if (!prefixes || !prefixes.length) return
    ///   this.restoreBefore(decl)
    ///   decl.raws.before = this.calcBefore(prefixes, decl)
    /// }
    /// ```
    pub fn process(&self, root: &mut Node, path: &[usize]) {
        // Tracks the path of the *original* decl through successive inserts.
        let mut current_path = path.to_vec();
        let parent = parent_prefix_cached_mut(root, &current_path);

        let prefixes: Vec<String> = self
            .prefixer
            .prefixes
            .iter()
            .filter(|p| match &parent {
                ParentPrefix::None => true,
                ParentPrefix::Some(s) => s == utils::remove_note(p),
            })
            .cloned()
            .collect();

        let need_cascade = {
            let here = match postcss_core::node_at_path_mut(root, &current_path)
            {
                Some(n) => n,
                None => return,
            };
            self.need_cascade(here)
        };

        let mut added: Vec<String> = Vec::new();
        for prefix in &prefixes {
            let mut so_far = added.clone();
            so_far.push(prefix.clone());
            if self.add(root, &current_path, prefix, &so_far).is_some() {
                added.push(prefix.clone());
                if let Some(last) = current_path.last_mut() {
                    *last += 1;
                }
            }
        }

        if !need_cascade || added.is_empty() {
            return;
        }
        // Re-flow the original decl's `raws.before` to the longest
        // prefix's column.
        let here = match postcss_core::node_at_path_mut(root, &current_path) {
            Some(n) => n,
            None => return,
        };
        // `calcBefore(prefixes, decl)` — the JS variant without an
        // explicit `prefix` arg uses `prefix=''`, which means
        // `removeNote('').length === 0` → diff === max.
        here.raws.before = Some(self.calc_before(&added, here, ""));
    }

    /// JS: `old(prop, prefix) { return [this.prefixed(prop, prefix)] }`.
    pub fn old(&self, prop: &str, prefix: &str) -> Vec<String> {
        vec![self.prefixed(prop, prefix)]
    }

    /// JS-side `_autoprefixerValues` map access (used by Value
    /// subclasses to merge into a decl's value list before flushing).
    pub fn values_map_mut(decl: &mut Node) -> Option<&mut IndexMap<String, String>> {
        decl.attrs.get_string_map_mut(crate::value::ATTR_VALUES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    fn first_decl_path() -> Vec<usize> {
        // root → rule(0) → decl(0).
        vec![0, 0]
    }

    #[test]
    fn prefixed_concatenates() {
        let d = DeclarationBase::new("flex".into(), vec![], 0);
        assert_eq!(d.prefixed("flex", "-webkit-"), "-webkit-flex");
    }

    #[test]
    fn other_prefixes_detects_other_vendor() {
        let d = DeclarationBase::new("flex".into(), vec![], 0);
        assert!(d.other_prefixes("-moz-foo", "-webkit-"));
        assert!(!d.other_prefixes("-webkit-foo", "-webkit-"));
    }

    #[test]
    fn other_prefixes_ignores_var_args() {
        let d = DeclarationBase::new("flex".into(), vec![], 0);
        // `-moz-foo` only inside `var(...)` should NOT count.
        assert!(!d.other_prefixes("var(-moz-foo)", "-webkit-"));
    }

    #[test]
    fn add_inserts_prefixed_clone() {
        let mut r = parse("a { display: flex; }").unwrap();
        let d = DeclarationBase::new("display".into(), vec!["-webkit-".into()], 0);
        d.add(&mut r.root, &first_decl_path(), "-webkit-", &["-webkit-".into()]);
        let out = stringify(&r);
        assert!(out.contains("-webkit-display: flex"));
        assert!(out.contains("display: flex"));
    }

    #[test]
    fn add_idempotent_when_prefixed_sibling_exists() {
        let mut r = parse(
            "a { -webkit-display: flex; display: flex; }",
        )
        .unwrap();
        let len_before = {
            let rule = &r.root.nodes().unwrap()[0];
            rule.nodes().unwrap().len()
        };
        let d = DeclarationBase::new("display".into(), vec!["-webkit-".into()], 0);
        // path [0, 1] points at the unprefixed `display: flex`.
        d.add(&mut r.root, &[0, 1], "-webkit-", &["-webkit-".into()]);
        let len_after = {
            let rule = &r.root.nodes().unwrap()[0];
            rule.nodes().unwrap().len()
        };
        assert_eq!(len_before, len_after);
    }

    #[test]
    fn process_emits_each_prefix_with_cursor_shift() {
        let mut r = parse("a { display: flex; }").unwrap();
        let d = DeclarationBase::new(
            "display".into(),
            vec!["-webkit-".into(), "-moz-".into()],
            0,
        );
        d.process(&mut r.root, &first_decl_path());
        let out = stringify(&r);
        assert!(out.contains("-webkit-display"));
        assert!(out.contains("-moz-display"));
        assert!(out.contains("display: flex"));
    }

    #[test]
    fn need_cascade_caches_decision() {
        let mut r = parse("a {\n  display: flex;\n}").unwrap();
        let d = DeclarationBase::new("display".into(), vec![], 0);
        let decl = {
            let rule = r.root.nodes_mut().unwrap().get_mut(0).unwrap();
            rule.nodes_mut().unwrap().get_mut(0).unwrap()
        };
        let result = d.need_cascade(decl);
        // `raws.before` for the first decl in `a {\n  display:...` is
        // `\n  ` which contains `\n` → cascade true.
        assert!(result);
        assert!(decl.attrs.contains(ATTR_CASCADE));
    }

    #[test]
    fn old_returns_single_prefixed_prop() {
        let d = DeclarationBase::new("display".into(), vec![], 0);
        assert_eq!(d.old("display", "-webkit-"), vec!["-webkit-display"]);
    }
}
