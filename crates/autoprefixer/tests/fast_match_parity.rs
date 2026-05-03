//! Property tests gating the hand-rolled `fast_match` matchers.
//!
//! These matchers replace `regex::Regex` calls inside `preprocess()`
//! and the prefixer hot paths. The byte-equality contract means the
//! matchers MUST produce identical results to Rust regex for every
//! input we care about. This file enforces that with:
//!
//!   1. **Hand-curated corner cases** — the obvious edge cases
//!      (empty haystack, NAME at start/end, multi-occurrence,
//!      adjacent NAMEs, embedded-in-larger-word).
//!   2. **Real-world NAME corpus** — every selector and value name
//!      autoprefixer's prefix table actually emits at runtime
//!      (`crates/autoprefixer/src/data/prefixes.rs`).
//!   3. **Pseudo-random fuzzing** — deterministic LCG over a wide
//!      character alphabet (ASCII + select Unicode whitespace, CSS
//!      punctuation, the most-likely "would-this-fold-to-ASCII"
//!      Unicode chars). Tens of thousands of generated samples per
//!      NAME.
//!
//! On failure the test reports the divergent input verbatim so the
//! offending edge case can be dropped into a fixed regression test.
//!
//! ## What this test does NOT prove
//!
//! - Correctness against patterns we haven't ported (this file only
//!   covers WORD and SELECTOR shapes — see `fast_match.rs` head).
//! - Performance — that's `crates/css/examples/perf_precomputed.rs`.
//!
//! Failure mode policy: if a divergence is found, the matcher must
//! either (a) be fixed AND retested, or (b) be reverted and the
//! relevant call site go back to the regex path. Patching the regex
//! to "match the matcher" is FORBIDDEN — that's the drift trap.

use autoprefixer::fast_match::{SelectorMatcher, WordMatcher};
use regex::Regex;

/// Build the Rust regex equivalent of the WORD pattern for `name`.
fn word_regex(name: &str) -> Regex {
    // Mirror `crates/autoprefixer/src/utils.rs::regexp(name, true)`:
    //   1. escape_regexp(name)
    //   2. format!("(?i)(^|[\\s,(])({}($|[\\s(,]))", escaped)
    let escaped = regex_escape(name);
    Regex::new(&format!(r"(?i)(^|[\s,(])({}($|[\s(,]))", escaped))
        .expect("word regex compiles")
}

/// Build the Rust regex equivalent of the SELECTOR pattern for `name`.
fn selector_regex(name: &str) -> Regex {
    let escaped = regex_escape(name);
    Regex::new(&format!(r#"(?i)(^|[^:"'=]){}"#, escaped))
        .expect("selector regex compiles")
}

/// Replicate `crates/autoprefixer/src/utils.rs::escape_regexp`.
fn regex_escape(s: &str) -> String {
    static CHARS: &[char] =
        &['$', '(', ')', '*', '+', '-', '.', '?', '[', '\\', ']', '^', '{', '|', '}'];
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if CHARS.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

// --------------------------------------------------------------------------
// Sample names (autoprefixer's actual identifier corpus)
// --------------------------------------------------------------------------

const WORD_NAMES: &[&str] = &[
    "flex",
    "inline-flex",
    "linear-gradient",
    "radial-gradient",
    "repeating-linear-gradient",
    "repeating-radial-gradient",
    "image-set",
    "cross-fade",
    "filter",
    "fit-content",
    "stretch",
    "fill",
    "fill-available",
    "min-content",
    "max-content",
    "calc",
    "element",
    "currentcolor",
    "transform",
    "rotate3d",
    "scale3d",
    "translate3d",
    // Prefixed forms (these flow through OldValue.regexp).
    "-webkit-flex",
    "-webkit-linear-gradient",
    "-moz-linear-gradient",
    "-o-linear-gradient",
    "-ms-linear-gradient",
    "-webkit-image-set",
    "-webkit-fill-available",
    "-moz-available",
];

const SELECTOR_NAMES: &[&str] = &[
    ":fullscreen",
    ":placeholder-shown",
    ":read-only",
    ":read-write",
    "::placeholder",
    "::backdrop",
    "::file-selector-button",
    "::selection",
    ":-webkit-fullscreen",
    ":-moz-fullscreen",
    ":-ms-fullscreen",
    "::-webkit-input-placeholder",
    "::-moz-placeholder",
    "::-ms-input-placeholder",
];

// --------------------------------------------------------------------------
// Hand-curated corner cases for WORD matcher
// --------------------------------------------------------------------------

const WORD_CORNER_HAYSTACKS: &[&str] = &[
    "",
    "flex",
    "Flex",
    "FLEX",
    "flexbox",
    "xflex",
    " flex",
    "flex ",
    " flex ",
    "flex,wrap",
    "wrap,flex",
    "(flex)",
    "flex(",
    "(flex",
    "flex(arg)",
    "display:flex", // colon NOT in left class — should not match
    "display: flex",
    "display:flex;",
    "linear-gradient(red, blue)",
    "flex flex flex",
    "flexflex",
    "flex flexbox",
    "flexbox flex",
    // Unicode haystacks
    "café flex",
    "flex café",
    "café-flex",
    "café\u{2003}flex", // U+2003 EM SPACE (whitespace per Unicode)
    "café\u{00A0}flex", // U+00A0 NBSP (whitespace)
    // Edge: leading whitespace
    "\tflex",
    "\nflex",
    // Repeats with different boundaries
    "(flex,flex,flex)",
    // Nasty embedded
    "var(--flex)",
    "url(flex.png)",
];

const SELECTOR_CORNER_HAYSTACKS: &[&str] = &[
    "",
    ":fullscreen",
    "a :fullscreen",
    "::fullscreen", // double-colon: leading char IS ':' — matcher must skip
    "='fullscreen'", // '=' is in NOT-allowed set
    "\":fullscreen\"",
    ":fullscreen:hover",
    ":fullscreen :fullscreen",
    "*:fullscreen",
    ".cls:fullscreen",
    "#id:fullscreen",
    "a, :fullscreen",
];

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[test]
fn word_matcher_corners() {
    for &name in WORD_NAMES {
        let matcher = WordMatcher::new(name);
        let regex = word_regex(name);
        for &h in WORD_CORNER_HAYSTACKS {
            let m_is = matcher.is_match(h);
            let r_is = regex.is_match(h);
            assert_eq!(
                m_is, r_is,
                "is_match drift: name={name:?} haystack={h:?}\n  matcher={m_is} regex={r_is}"
            );

            // Exercise replace_all with several prefixes.
            for prefix in &["-webkit-", "-moz-", "-ms-", "-o-", ""] {
                let m_out = matcher.replace_all_with_prefix(h, prefix);
                let r_out = replace_all_word_with_prefix(&regex, h, prefix);
                assert_eq!(
                    m_out, r_out,
                    "replace_all drift: name={name:?} prefix={prefix:?} haystack={h:?}\n  \
                     matcher={m_out:?}\n  regex={r_out:?}"
                );
            }
        }
    }
}

#[test]
fn selector_matcher_corners() {
    for &name in SELECTOR_NAMES {
        let matcher = SelectorMatcher::new(name);
        let regex = selector_regex(name);
        for &h in SELECTOR_CORNER_HAYSTACKS {
            let m_is = matcher.is_match(h);
            let r_is = regex.is_match(h);
            assert_eq!(
                m_is, r_is,
                "selector is_match drift: name={name:?} haystack={h:?}\n  matcher={m_is} regex={r_is}"
            );

            // Exercise replace_all with prefixed selector replacement.
            for replacement in &[":-webkit-fullscreen", ":-moz-test", ""] {
                let m_out = matcher.replace_all_with(h, replacement);
                let r_out = replace_all_selector_with(&regex, h, replacement);
                assert_eq!(
                    m_out, r_out,
                    "selector replace drift: name={name:?} repl={replacement:?} haystack={h:?}\n  \
                     matcher={m_out:?}\n  regex={r_out:?}"
                );
            }
        }
    }
}

#[test]
fn word_matcher_fuzz() {
    // Deterministic LCG so failures are reproducible.
    let mut rng = Lcg::new(0xCAFE_BABE_F00D_5EED);
    const ITERS: u32 = 20_000;

    for &name in WORD_NAMES {
        let matcher = WordMatcher::new(name);
        let regex = word_regex(name);

        for _ in 0..ITERS {
            let haystack = rng.gen_haystack(name);

            let m_is = matcher.is_match(&haystack);
            let r_is = regex.is_match(&haystack);
            if m_is != r_is {
                panic!(
                    "is_match drift (fuzz): name={name:?} haystack={haystack:?}\n  matcher={m_is} regex={r_is}"
                );
            }

            let prefix = "-webkit-";
            let m_out = matcher.replace_all_with_prefix(&haystack, prefix);
            let r_out = replace_all_word_with_prefix(&regex, &haystack, prefix);
            if m_out != r_out {
                panic!(
                    "replace_all drift (fuzz): name={name:?} haystack={haystack:?}\n  \
                     matcher={m_out:?}\n  regex={r_out:?}"
                );
            }
        }
    }
}

#[test]
fn selector_matcher_fuzz() {
    let mut rng = Lcg::new(0x1234_5678_DEAD_BEEF);
    const ITERS: u32 = 20_000;

    for &name in SELECTOR_NAMES {
        let matcher = SelectorMatcher::new(name);
        let regex = selector_regex(name);

        for _ in 0..ITERS {
            let haystack = rng.gen_haystack(name);

            let m_is = matcher.is_match(&haystack);
            let r_is = regex.is_match(&haystack);
            if m_is != r_is {
                panic!(
                    "selector is_match drift (fuzz): name={name:?} haystack={haystack:?}\n  matcher={m_is} regex={r_is}"
                );
            }

            let replacement = ":-webkit-fullscreen";
            let m_out = matcher.replace_all_with(&haystack, replacement);
            let r_out = replace_all_selector_with(&regex, &haystack, replacement);
            if m_out != r_out {
                panic!(
                    "selector replace drift (fuzz): name={name:?} haystack={haystack:?}\n  \
                     matcher={m_out:?}\n  regex={r_out:?}"
                );
            }
        }
    }
}

// --------------------------------------------------------------------------
// Reference implementations of the regex `replace_all` semantics.
// --------------------------------------------------------------------------

/// Mirror `re.replace_all(s, |caps| format!("{}{prefix}{}", caps[1], caps[2]))`.
fn replace_all_word_with_prefix(re: &Regex, haystack: &str, prefix: &str) -> String {
    re.replace_all(haystack, |caps: &regex::Captures| {
        format!(
            "{}{}{}",
            caps.get(1).map(|m| m.as_str()).unwrap_or(""),
            prefix,
            caps.get(2).map(|m| m.as_str()).unwrap_or(""),
        )
    })
    .into_owned()
}

/// Mirror `re.replace_all(s, |caps| format!("{}{replacement}", caps[1]))`.
fn replace_all_selector_with(re: &Regex, haystack: &str, replacement: &str) -> String {
    re.replace_all(haystack, |caps: &regex::Captures| {
        format!(
            "{}{}",
            caps.get(1).map(|m| m.as_str()).unwrap_or(""),
            replacement,
        )
    })
    .into_owned()
}

// --------------------------------------------------------------------------
// Deterministic fuzz generator
// --------------------------------------------------------------------------

/// A small Linear Congruential Generator. Keeps the suite deterministic
/// (a seed → a corpus). Numerical Recipes constants.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    fn gen_range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        let span = hi - lo;
        ((self.next_u64() >> 32) as u32 % span) + lo
    }

    /// Build a haystack with a high probability of containing `name`
    /// (so we exercise both match and non-match paths densely).
    fn gen_haystack(&mut self, name: &str) -> String {
        // Char alphabet — a mix of ASCII identifier chars, CSS
        // punctuation (boundary chars), and selected Unicode chars
        // that exercise non-ASCII walking.
        const ALPHABET: &[char] = &[
            'a', 'b', 'c', 'd', 'e', 'f', 'g', 'l', 'm', 'n', 'r', 't', 'x',
            'A', 'B', 'C', 'D', 'E', 'F', 'L', 'M', 'X',
            '0', '1', '2', '9',
            ' ', '\t', '\n', ',', '(', ')', ':', ';', '"', '\'', '=', '-',
            '.', '#', '*', '/', '%', '<', '>',
            // Non-ASCII probes
            'é', 'ä', '中', '日', '🎨',
            '\u{00A0}', // NBSP — Unicode whitespace
            '\u{2003}', // EM SPACE — Unicode whitespace
            '\u{2028}', // LINE SEPARATOR — Unicode whitespace
            // Latin Capital Letter I With Dot Above — case-folding
            // sentinel; if regex matches `i`/`I` against `İ` the
            // matchers will diverge here.
            '\u{0130}',
        ];

        let len = self.gen_range(0, 40) as usize;
        let mut out = String::new();

        // 60% of the time, embed `name` somewhere — possibly multiple
        // times. This dramatically increases match-path coverage.
        let embed = (self.next_u64() & 0xff) < 153; // ~60%
        let embed_count = if embed { self.gen_range(1, 4) } else { 0 };

        for _ in 0..len {
            // 5% chance to insert a name at this position (case-randomized).
            if embed_count > 0 && (self.next_u64() & 0xff) < 12 {
                out.push_str(&self.case_randomize(name));
                continue;
            }
            let idx = self.gen_range(0, ALPHABET.len() as u32) as usize;
            out.push(ALPHABET[idx]);
        }

        // Ensure at least one embedding actually lands when requested.
        for _ in 0..embed_count.saturating_sub(1) {
            let pos = self.gen_range(0, (out.chars().count() + 1) as u32) as usize;
            let chars: Vec<char> = out.chars().collect();
            let mut new_str = String::new();
            for (i, c) in chars.iter().enumerate() {
                if i == pos {
                    new_str.push_str(&self.case_randomize(name));
                }
                new_str.push(*c);
            }
            if pos == chars.len() {
                new_str.push_str(&self.case_randomize(name));
            }
            out = new_str;
        }

        out
    }

    fn case_randomize(&mut self, s: &str) -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphabetic() && (self.next_u64() & 1) == 0 {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            })
            .collect()
    }
}
