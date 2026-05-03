//! Integration tests for `transition.rs`. Mirrors the in-file unit tests
//! but lives at the integration layer so they can run while AGENT_2's
//! `supports.rs` test code still references the pre-AGENT_1 `Prefixes`
//! struct shape (which is currently a lib-test compile error — see
//! `AGENT_3_DONE.md` "Drift detected" section).

use autoprefixer::transition::{
    FlexboxOption, Transition, TransitionPrefixesView, TRANSITION_PROPS,
};
use autoprefixer::vendor;
use indexmap::IndexMap;
use postcss_core::{parse, stringify};
use postcss_value_parser::{parse as vp_parse, NodeKind as VNodeKind};

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
    vec![0, 0]
}

#[test]
fn props_are_transition_and_transition_property() {
    let mock = MockPrefixes::default();
    let t = Transition::new(&mock);
    assert_eq!(t.props, TRANSITION_PROPS);
    assert_eq!(t.props, &["transition", "transition-property"]);
}

// --------------------- find_prop ---------------------

#[test]
fn find_prop_returns_first_word() {
    let nodes = vp_parse("transform 0.3s ease");
    assert_eq!(Transition::find_prop(&nodes), "transform");
}

#[test]
fn find_prop_handles_leading_duration() {
    let nodes = vp_parse("0.3s transform ease");
    assert_eq!(Transition::find_prop(&nodes), "transform");
}

#[test]
fn find_prop_falls_back_to_first_value_when_no_word_after() {
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
    let last = params[0].last().unwrap();
    assert_eq!(last.kind, VNodeKind::Div);
    assert_eq!(last.value, ",");
}

#[test]
fn parse_value_filters_empty_params() {
    let mock = MockPrefixes::default();
    let t = Transition::new(&mock);
    let params = t.parse_value("transform 0.3s,");
    assert_eq!(params.len(), 1);
}

#[test]
fn stringify_params_empty_returns_empty_string() {
    let mock = MockPrefixes::default();
    let t = Transition::new(&mock);
    let mut params: Vec<Vec<postcss_value_parser::Node>> = Vec::new();
    assert_eq!(t.stringify_params(&mut params), "");
}

#[test]
fn stringify_params_round_trips_simple_value() {
    let mock = MockPrefixes::default();
    let t = Transition::new(&mock);
    let mut params = t.parse_value("transform 0.3s ease");
    assert_eq!(t.stringify_params(&mut params), "transform 0.3s ease");
}

// --------------------- clone_param ---------------------

#[test]
fn clone_param_replaces_first_matching_word() {
    let nodes = vp_parse("transform 0.3s ease");
    let cloned = Transition::clone_param("transform", "-webkit-transform", &nodes);
    assert_eq!(cloned.first().unwrap().value, "-webkit-transform");
    assert_eq!(cloned[2].value, "0.3s");
}

#[test]
fn clone_param_only_replaces_once() {
    let nodes = vp_parse("transform transform");
    let cloned = Transition::clone_param("transform", "-webkit-transform", &nodes);
    assert_eq!(cloned[0].value, "-webkit-transform");
    assert_eq!(cloned[2].value, "transform");
}

// --------------------- clean_from_unprefixed ---------------------

#[test]
fn clean_from_unprefixed_drops_unprefixed_when_prefixed_present() {
    let mock = MockPrefixes::default();
    let t = Transition::new(&mock);
    let params = t.parse_value("transform 0.3s, -webkit-transform 0.3s");
    let filtered = t.clean_from_unprefixed(&params, "-webkit-");
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
    assert!(t
        .rule_vendor_prefixes(&r.root, &first_decl_path())
        .is_none());
}

#[test]
fn rule_vendor_prefixes_detects_vendor_pseudo() {
    let mock = MockPrefixes::default();
    let t = Transition::new(&mock);
    let r = parse(":-webkit-full-screen { transition: transform 0.3s; }").unwrap();
    let res = t.rule_vendor_prefixes(&r.root, &first_decl_path());
    assert!(res.is_some());
    let prefixes = res.unwrap();
    assert!(prefixes.iter().any(|p| p == "-webkit-"));
}

// --------------------- add ---------------------

#[test]
fn add_no_prefixed_props_is_noop() {
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
    let mock = MockPrefixes::default().with_add("transform", &["-webkit-"]);
    let t = Transition::new(&mock);
    let mut r = parse("a { transition: transform 0.3s ease; }").unwrap();
    let mut warnings: Vec<String> = Vec::new();
    t.add(&mut r.root, &first_decl_path(), &mut warnings);
    let out = stringify(&r);
    assert!(
        out.contains("transform 0.3s ease, -webkit-transform 0.3s ease"),
        "expected combined value list in: {out}"
    );
    assert!(
        out.contains("transition: -webkit-transform 0.3s ease"),
        "expected webkit-only fallback decl in: {out}"
    );
}

#[test]
fn add_inserts_webkit_transition_when_decl_has_webkit() {
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
    let mock = MockPrefixes::default().with_add("transform", &["-webkit-"]);
    let t = Transition::new(&mock);
    let css = "a { transition: -webkit-transform 0.3s ease; }";
    let mut r = parse(css).unwrap();
    let mut warnings: Vec<String> = Vec::new();
    t.add(&mut r.root, &first_decl_path(), &mut warnings);
    let out = stringify(&r);
    assert_eq!(out, css);
}

#[test]
fn add_skips_ms_transform_prefix() {
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
    // ≥2 prefixes on the same decl exercises the cursor-shift bump.
    let mock = MockPrefixes::default()
        .with_add("transition", &["-webkit-", "-o-"])
        .with_add("transform", &["-webkit-", "-o-"]);
    let t = Transition::new(&mock);
    let mut r = parse("a { transition: transform 0.3s ease; }").unwrap();
    let mut warnings: Vec<String> = Vec::new();
    t.add(&mut r.root, &first_decl_path(), &mut warnings);
    let out = stringify(&r);
    assert!(
        out.contains("-webkit-transition:"),
        "expected -webkit-transition in: {out}"
    );
    assert!(
        out.contains("-o-transition:"),
        "expected -o-transition in: {out}"
    );
}

// --------------------- remove ---------------------

#[test]
fn remove_drops_marked_param_and_updates_value() {
    let mock = MockPrefixes::default().with_remove("-webkit-transform");
    let t = Transition::new(&mock);
    let mut r =
        parse("a { transition: transform 0.3s ease, -webkit-transform 0.3s ease; }")
            .unwrap();
    t.remove(&mut r.root, &first_decl_path());
    let out = stringify(&r);
    assert!(out.contains("transform 0.3s ease"));
    assert!(!out.contains("-webkit-transform 0.3s ease"));
}

#[test]
fn remove_removes_decl_when_all_params_filtered() {
    let mock = MockPrefixes::default().with_remove("-webkit-transform");
    let t = Transition::new(&mock);
    let mut r = parse("a { transition: -webkit-transform 0.3s ease; }").unwrap();
    t.remove(&mut r.root, &first_decl_path());
    let out = stringify(&r);
    assert!(!out.contains("transition"), "expected decl removed: {out}");
}

// --------------------- check_for_warning (via add) ---------------------

// `check_for_warning` is private — we exercise it indirectly via the JS-
// matching condition through `add`. The dedicated unit test lives in
// `transition.rs::tests::check_for_warning_*`; they will run once the
// supports.rs drift is resolved and lib-tests compile again.
