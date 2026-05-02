//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/resolution.js`.
//!
//! `class Resolution extends Prefixer`. Handles `@media (min-resolution: ...)`
//! / `@media (max-resolution: ...)` prefixing — `-webkit-`/`-o-`/`-moz-`
//! variants use device-pixel-ratio and (for `-o-`) `n/d` fraction syntax.

use fraction_js::fraction::Fraction;
use postcss_core::{Node, NodeKind};
use regex::Regex;

use crate::prefixer::{parent_prefix_cached_mut, ParentPrefix, PrefixerBase};
use crate::utils;

static REGEXP: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
    Regex::new(r"(?i)(min|max)-resolution\s*:\s*\d*\.?\d+(dppx|dpcm|dpi|x)")
        .unwrap()
});
static SPLIT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
    Regex::new(r"(?i)(min|max)-resolution(\s*:\s*)(\d*\.?\d+)(dppx|dpcm|dpi|x)")
        .unwrap()
});

pub struct ResolutionBase {
    pub prefixer: PrefixerBase,
    pub bad: Vec<String>,
}

impl ResolutionBase {
    pub fn new(name: String, prefixes: Vec<String>, all_id: usize) -> Self {
        Self {
            prefixer: PrefixerBase::new(name, prefixes, all_id),
            bad: Vec::new(),
        }
    }

    /// JS: `prefixName(prefix, name)`.
    /// ```js
    /// if (prefix === '-moz-') return name + '--moz-device-pixel-ratio'
    /// else return prefix + name + '-device-pixel-ratio'
    /// ```
    pub fn prefix_name(&self, prefix: &str, name: &str) -> String {
        if prefix == "-moz-" {
            format!("{name}--moz-device-pixel-ratio")
        } else {
            format!("{prefix}{name}-device-pixel-ratio")
        }
    }

    /// JS: `prefixQuery(prefix, name, colon, value, units)`.
    /// Converts dpi/dpcm/dppx → device-pixel-ratio, then formats:
    /// - `-o-` prefix: emits `n/d` fraction syntax.
    /// - others: emits decimal.
    pub fn prefix_query(
        &self,
        prefix: &str,
        name: &str,
        colon: &str,
        value: &str,
        units: &str,
    ) -> String {
        // 1dpcm = 2.54dpi; 1dppx = 96dpi.
        let f = Fraction::new(value).expect("valid numeric input");
        let f = match units {
            "dpi" => f.div(96.0).expect("non-zero divisor"),
            "dpcm" => {
                f.mul(2.54).expect("ok").div(96.0).expect("non-zero divisor")
            }
            // "dppx" / "x" — already in device-pixel-ratio.
            _ => f,
        };
        // JS calls `f.simplify()` here. The Rust port already keeps n/d
        // in reduced form via `new_fraction`, so simplify is a no-op.
        let value_str = if prefix == "-o-" {
            format!(
                "{}/{}",
                postcss_core::js_number_to_string(f.n * f.s),
                postcss_core::js_number_to_string(f.d),
            )
        } else {
            postcss_core::js_number_to_string(f.s * f.n / f.d)
        };
        format!("{}{colon}{value_str}", self.prefix_name(prefix, name))
    }

    /// JS: `clean(rule)` — strip prefixed query params from `rule.params`.
    pub fn clean(&mut self, rule: &mut Node) {
        if self.bad.is_empty() {
            for prefix in self.prefixer.prefixes.clone().iter() {
                self.bad.push(self.prefix_name(prefix, "min"));
                self.bad.push(self.prefix_name(prefix, "max"));
            }
        }
        let bad = self.bad.clone();
        if let NodeKind::AtRule(at) = &mut rule.kind {
            at.params = utils::edit_list(&at.params, |queries| {
                queries
                    .into_iter()
                    .filter(|q| bad.iter().all(|b| !q.contains(b)))
                    .collect()
            });
        }
    }

    /// JS: `process(rule)`. Walk `rule.params` (a comma-list of media
    /// queries), for any query mentioning `min/max-resolution`, emit a
    /// prefixed sibling for each prefix, then keep the original.
    pub fn process(&mut self, root: &mut Node, path: &[usize]) {
        let parent = parent_prefix_cached_mut(root, path);
        let prefixes: Vec<String> = match &parent {
            ParentPrefix::None => self.prefixer.prefixes.clone(),
            ParentPrefix::Some(s) => vec![s.clone()],
        };

        let here = match postcss_core::node_at_path_mut(root, path) {
            Some(n) => n,
            None => return,
        };
        let params = match &here.kind {
            NodeKind::AtRule(at) => at.params.clone(),
            _ => return,
        };
        let new_params = utils::edit_list(&params, |origin| {
            let mut prefixed: Vec<String> = Vec::new();
            for query in origin.iter() {
                if !query.contains("min-resolution")
                    && !query.contains("max-resolution")
                {
                    prefixed.push(query.clone());
                    continue;
                }

                for prefix in &prefixes {
                    let processed =
                        REGEXP.replace_all(query, |caps: &regex::Captures| {
                            let s = caps.get(0).unwrap().as_str();
                            let parts = SPLIT.captures(s).expect(
                                "REGEXP and SPLIT match the same shape",
                            );
                            self.prefix_query(
                                prefix,
                                parts.get(1).unwrap().as_str(),
                                parts.get(2).unwrap().as_str(),
                                parts.get(3).unwrap().as_str(),
                                parts.get(4).unwrap().as_str(),
                            )
                        });
                    prefixed.push(processed.into_owned());
                }
                prefixed.push(query.clone());
            }
            utils::uniq(&prefixed)
        });

        if let NodeKind::AtRule(ref mut at) = here.kind {
            at.params = new_params;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_name_moz_special_case() {
        let r = ResolutionBase::new("media".into(), vec![], 0);
        assert_eq!(
            r.prefix_name("-moz-", "min"),
            "min--moz-device-pixel-ratio"
        );
        assert_eq!(
            r.prefix_name("-webkit-", "max"),
            "-webkit-max-device-pixel-ratio"
        );
    }

    #[test]
    fn prefix_query_dpi_conversion() {
        let r = ResolutionBase::new("media".into(), vec![], 0);
        // 192dpi / 96 = 2 → "-webkit-min-device-pixel-ratio: 2".
        let out = r.prefix_query("-webkit-", "min", ": ", "192", "dpi");
        assert_eq!(out, "-webkit-min-device-pixel-ratio: 2");
    }

    #[test]
    fn prefix_query_o_emits_fraction_syntax() {
        let r = ResolutionBase::new("media".into(), vec![], 0);
        // 192dpi / 96 = 2/1 (in `-o-`-fraction form).
        let out = r.prefix_query("-o-", "min", ": ", "192", "dpi");
        assert_eq!(out, "-o-min-device-pixel-ratio: 2/1");
    }

    #[test]
    fn prefix_query_dppx_passthrough() {
        let r = ResolutionBase::new("media".into(), vec![], 0);
        // dppx is already device-pixel-ratio — no math needed.
        let out = r.prefix_query("-webkit-", "min", ": ", "2", "dppx");
        assert_eq!(out, "-webkit-min-device-pixel-ratio: 2");
    }
}
