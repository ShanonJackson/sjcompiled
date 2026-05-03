//! Property tests gating `IntrinsicMatcher` / `IntrinsicRegexp` —
//! parallel to `fast_match_parity.rs` for the WORD/SELECTOR matchers.
//!
//! The Intrinsic regex differs from the WORD regex by a single byte
//! in the trailing-boundary class: `[\s),]` (Intrinsic) vs `[\s(,]`
//! (WORD). The byte that flips is `(`. That single difference is the
//! entire reason `IntrinsicRegexp` exists.
//!
//! ## Drift discipline
//!
//! These matchers MUST be byte-equal to `regex::Regex` for every
//! input on the Intrinsic-named corpus. If a divergence ever surfaces,
//! REVERT to the regex path — DO NOT patch the matcher. See
//! `fast_match_parity.rs` head for the policy.
//!
//! ## What this test pins
//!
//! 1. **Corner cases** — including the canonical trap: `fit-content(`
//!    MUST match WORD but MUST NOT match Intrinsic. If a future change
//!    to the matcher accidentally folds the trailing class back to
//!    WORD's, this test fires.
//! 2. **Real Intrinsic name corpus** — the 6 hack names plus their
//!    vendor-prefixed forms (`-moz-available`, `-webkit-fill-available`).
//! 3. **20k LCG fuzz iters per name** vs `regex::Regex`.

use autoprefixer::fast_match::{IntrinsicMatcher, IntrinsicRegexp};
use regex::Regex;

/// Build the regex equivalent of the Intrinsic pattern for `name`.
/// Mirrors `crates/autoprefixer/src/hacks/intrinsic.rs::intrinsic_regexp`:
///   (?i)(^|[\s,(])({}($|[\s),]))
fn intrinsic_regex(name: &str) -> Regex {
    let escaped = regex_escape(name);
    Regex::new(&format!(r"(?i)(^|[\s,(])({}($|[\s),]))", escaped))
        .expect("intrinsic regex compiles")
}

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
// Names — the autoprefixer Intrinsic hack corpus + vendor-prefixed forms.
// --------------------------------------------------------------------------

const INTRINSIC_NAMES: &[&str] = &[
    "max-content",
    "min-content",
    "fit-content",
    "fill",
    "fill-available",
    "stretch",
    // Vendor-prefixed forms used by `Intrinsic::old(prefix)`.
    "-webkit-max-content",
    "-webkit-min-content",
    "-webkit-fit-content",
    "-moz-max-content",
    "-moz-min-content",
    "-moz-fit-content",
    "-webkit-fill-available",
    "-moz-available",
    "-webkit-stretch",
];

// --------------------------------------------------------------------------
// Hand-curated corner cases — pin every single edge case where Intrinsic's
// trailing-boundary class `[\s),]` differs from WORD's `[\s(,]`.
// --------------------------------------------------------------------------

const INTRINSIC_CORNER_HAYSTACKS: &[&str] = &[
    // Empty haystack
    "",
    // The CANONICAL trap — `name(` matches WORD but MUST NOT match
    // Intrinsic. If this drifts, AFM `width: max(fit-content(...))`
    // and similar `*()`-shaped values produce the wrong bytes.
    "fit-content(",
    "max-content(",
    "min-content(",
    "fill(",
    "stretch(",
    // The mirror — `name)` MATCHES Intrinsic but NOT WORD. The other
    // single-byte difference between the two classes.
    "fit-content)",
    "max-content)",
    "fill)",
    "stretch)",
    // Trailing comma — both classes contain `,`, so MATCHES both.
    "fit-content,",
    "fit-content,wrap",
    // Trailing whitespace — matches.
    "fit-content ",
    "fit-content\t",
    // Trailing end-of-string — matches via `$` alternation.
    "fit-content",
    "stretch",
    // Leading boundary chars (same class as WORD): whitespace / `,` / `(`.
    " fit-content",
    ",fit-content",
    "(fit-content",
    "(fit-content)",
    // Leading char NOT in left class — must NOT match.
    "xfit-content", // letter prefix rejection
    "1fit-content", // digit prefix rejection
    ":fit-content", // colon — NOT in left class
    ".fit-content",
    "/fit-content",
    "[fit-content", // `[` rejection — not in either class
    // Trailing char NOT in right class — must NOT match.
    "fit-contentx", // letter suffix rejection
    "fit-content1", // digit suffix rejection
    "fit-content[", // `[` rejection on right
    "fit-content;", // `;` not in right class
    "fit-content=",
    "fit-content<",
    // Embedded — neither boundary present.
    "afit-contentb",
    "1fit-content2",
    // Case-folding — Intrinsic regex has `(?i)`.
    "FIT-CONTENT",
    "Fit-Content",
    " FIT-CONTENT ",
    // Multi-occurrence — non-overlapping consumption of boundary chars.
    "fit-content fit-content fit-content",
    "fit-content,fit-content,fit-content",
    "(fit-content,fit-content)",
    // `name) name(` — first matches (right=`)`), second does NOT
    // (right=`(`). Tests the asymmetric trailing class on adjacent
    // tokens.
    "fit-content) fit-content(",
    // Unicode haystacks — non-ASCII chars must NOT count as boundaries
    // unless whitespace.
    "café fit-content",
    "fit-content café",
    "café-fit-content",
    "café\u{2003}fit-content", // EM SPACE — Unicode whitespace
    "café\u{00A0}fit-content", // NBSP — Unicode whitespace
    // Embedded in CSS-shaped values.
    "width: fit-content",
    "width: fit-content;",
    "width: fit-content)",
    "calc(fit-content + 10px)",
    "max(fit-content, 100px)",
    // Whitespace boundary variants.
    "\tfit-content",
    "\nfit-content",
    "fit-content\n",
    "fit-content\r\n",
];

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[test]
fn intrinsic_matcher_corners() {
    for &name in INTRINSIC_NAMES {
        let matcher = IntrinsicMatcher::new(name);
        let regex = intrinsic_regex(name);
        for &h in INTRINSIC_CORNER_HAYSTACKS {
            let m_is = matcher.is_match(h);
            let r_is = regex.is_match(h);
            assert_eq!(
                m_is, r_is,
                "is_match drift: name={name:?} haystack={h:?}\n  matcher={m_is} regex={r_is}"
            );

            for prefix in &["-webkit-", "-moz-", "-ms-", "-o-", ""] {
                let m_out = matcher.replace_all_with_prefix(h, prefix);
                let r_out = replace_all_intrinsic_with_prefix(&regex, h, prefix);
                assert_eq!(
                    m_out, r_out,
                    "replace_all_with_prefix drift: name={name:?} prefix={prefix:?} haystack={h:?}\n  \
                     matcher={m_out:?}\n  regex={r_out:?}"
                );
            }

            for alias in &["-moz-available", "-webkit-fill-available", ""] {
                let m_out = matcher.replace_all_with_vendor_alias(h, alias);
                let r_out =
                    replace_all_intrinsic_with_vendor_alias(&regex, h, alias);
                assert_eq!(
                    m_out, r_out,
                    "replace_all_with_vendor_alias drift: name={name:?} alias={alias:?} haystack={h:?}\n  \
                     matcher={m_out:?}\n  regex={r_out:?}"
                );
            }
        }
    }
}

/// The canonical trap, isolated as a single named test so a drift here
/// is unmistakable in CI output. `fit-content(` MUST NOT match Intrinsic
/// — if it does, the matcher has folded back to WORD semantics.
#[test]
fn fit_content_open_paren_must_not_match() {
    let matcher = IntrinsicMatcher::new("fit-content");
    let regex = intrinsic_regex("fit-content");
    assert!(
        !matcher.is_match("fit-content("),
        "IntrinsicMatcher MUST NOT match `fit-content(` — `(` is not in `[\\s),]`"
    );
    assert!(
        !regex.is_match("fit-content("),
        "regex sanity check — Intrinsic regex must not match `fit-content(`"
    );
}

/// Mirror — confirms the MUST-match side of the asymmetric class.
#[test]
fn fit_content_close_paren_must_match() {
    let matcher = IntrinsicMatcher::new("fit-content");
    let regex = intrinsic_regex("fit-content");
    assert!(
        matcher.is_match("fit-content)"),
        "IntrinsicMatcher MUST match `fit-content)` — `)` IS in `[\\s),]`"
    );
    assert!(regex.is_match("fit-content)"));
}

#[test]
fn intrinsic_regexp_wrapper_matches_matcher() {
    // The IntrinsicRegexp wrapper exists so OldValueRegexp::Intrinsic
    // can route through the same fast-match path. Sanity-check that
    // the wrapper produces the same bytes as the bare matcher does.
    for &name in INTRINSIC_NAMES {
        let matcher = IntrinsicMatcher::new(name);
        let wrapper = IntrinsicRegexp::new(name);
        for &h in INTRINSIC_CORNER_HAYSTACKS {
            assert_eq!(
                matcher.is_match(h),
                wrapper.is_match(h),
                "wrapper diverges from matcher: name={name:?} haystack={h:?}"
            );
        }
    }
}

#[test]
fn intrinsic_matcher_fuzz() {
    // Deterministic LCG so failures are reproducible.
    let mut rng = Lcg::new(0xDEAD_BEEF_F00D_5EED);
    const ITERS: u32 = 20_000;

    for &name in INTRINSIC_NAMES {
        let matcher = IntrinsicMatcher::new(name);
        let regex = intrinsic_regex(name);

        for _ in 0..ITERS {
            let haystack = rng.gen_haystack(name);

            let m_is = matcher.is_match(&haystack);
            let r_is = regex.is_match(&haystack);
            if m_is != r_is {
                panic!(
                    "is_match drift (fuzz): name={name:?} haystack={haystack:?}\n  matcher={m_is} regex={r_is}"
                );
            }

            for prefix in &["-webkit-", "-moz-"] {
                let m_out = matcher.replace_all_with_prefix(&haystack, prefix);
                let r_out =
                    replace_all_intrinsic_with_prefix(&regex, &haystack, prefix);
                if m_out != r_out {
                    panic!(
                        "replace_all_with_prefix drift (fuzz): name={name:?} prefix={prefix:?} haystack={haystack:?}\n  \
                         matcher={m_out:?}\n  regex={r_out:?}"
                    );
                }
            }

            for alias in &["-moz-available", "-webkit-fill-available"] {
                let m_out =
                    matcher.replace_all_with_vendor_alias(&haystack, alias);
                let r_out =
                    replace_all_intrinsic_with_vendor_alias(&regex, &haystack, alias);
                if m_out != r_out {
                    panic!(
                        "replace_all_with_vendor_alias drift (fuzz): name={name:?} alias={alias:?} haystack={haystack:?}\n  \
                         matcher={m_out:?}\n  regex={r_out:?}"
                    );
                }
            }
        }
    }
}

// --------------------------------------------------------------------------
// Reference implementations of the regex `replace_all` semantics.
// --------------------------------------------------------------------------

/// Mirror `regex.replace_all(s, |caps| format!("{}{prefix}{}", caps[1], caps[2]))`
/// — JS-side `Intrinsic::replace` non-stretch branch.
fn replace_all_intrinsic_with_prefix(re: &Regex, haystack: &str, prefix: &str) -> String {
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

/// Mirror `regex.replace_all(s, |caps| format!("{}{alias}{}", caps[1], caps[3]))`
/// — JS-side `Intrinsic::replace` stretch-family branch (drops NAME,
/// keeps leading + trailing boundaries).
fn replace_all_intrinsic_with_vendor_alias(
    re: &Regex,
    haystack: &str,
    alias: &str,
) -> String {
    re.replace_all(haystack, |caps: &regex::Captures| {
        format!(
            "{}{}{}",
            caps.get(1).map(|m| m.as_str()).unwrap_or(""),
            alias,
            caps.get(3).map(|m| m.as_str()).unwrap_or(""),
        )
    })
    .into_owned()
}

// --------------------------------------------------------------------------
// Deterministic fuzz generator — copy of the LCG / haystack shape from
// `fast_match_parity.rs` (kept inline so the two property suites stay
// independently auditable).
// --------------------------------------------------------------------------

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn gen_range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        let span = hi - lo;
        ((self.next_u64() >> 32) as u32 % span) + lo
    }

    fn gen_haystack(&mut self, name: &str) -> String {
        const ALPHABET: &[char] = &[
            'a', 'b', 'c', 'd', 'e', 'f', 'g', 'l', 'm', 'n', 'r', 't', 'x',
            'A', 'B', 'C', 'D', 'E', 'F', 'L', 'M', 'X',
            '0', '1', '2', '9',
            ' ', '\t', '\n', ',', '(', ')', ':', ';', '"', '\'', '=', '-',
            '.', '#', '*', '/', '%', '<', '>', '[', ']',
            'é', 'ä', '中', '日', '🎨',
            '\u{00A0}',
            '\u{2003}',
            '\u{2028}',
            '\u{0130}',
        ];

        let len = self.gen_range(0, 40) as usize;
        let mut out = String::new();

        let embed = (self.next_u64() & 0xff) < 153; // ~60%
        let embed_count = if embed { self.gen_range(1, 4) } else { 0 };

        for _ in 0..len {
            if embed_count > 0 && (self.next_u64() & 0xff) < 12 {
                out.push_str(&self.case_randomize(name));
                continue;
            }
            let idx = self.gen_range(0, ALPHABET.len() as u32) as usize;
            out.push(ALPHABET[idx]);
        }
        out
    }

    fn case_randomize(&mut self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if c.is_ascii_alphabetic() && (self.next_u64() & 1) == 1 {
                out.push(c.to_ascii_uppercase());
            } else {
                out.push(c);
            }
        }
        out
    }
}
