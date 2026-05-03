//! Hand-rolled byte-equal substitutes for the two regex shapes that
//! dominate `Prefixes::preprocess()` time (~93% of preprocess at 111
//! compiles per call, profiled in `crates/css/examples/perf_precomputed.rs`).
//!
//! ## Drift discipline
//!
//! These matchers are PURE PERFORMANCE PORTS. They MUST be byte-equal
//! to the Rust `regex::Regex` they replace on every input — that
//! contract is enforced by the property tests in `tests/` (huge fuzz
//! corpus + real CSS samples + corner cases). The full parity-runner
//! corpus (337 fixtures) is the merge gate.
//!
//! If a divergence is ever found, **revert to the regex path for that
//! call site** rather than patch the matcher — patching here adds drift
//! that won't show up until production. The matchers are scoped to
//! produce identical bytes OR fall back; never to "almost match".
//!
//! ## ASCII-only invariant
//!
//! NAME (the autoprefixer-side identifier — e.g. `flex`, `:fullscreen`,
//! `linear-gradient`) is always ASCII. CSS identifiers per
//! https://www.w3.org/TR/css-syntax-3/#ident-token-diagram permit
//! non-ASCII, but autoprefixer's prefix table (`crates/autoprefixer/src/data/prefixes.rs`)
//! never contains non-ASCII names — every entry is built from ASCII
//! CSS feature names.
//!
//! Haystacks (declaration values, selectors) **can** contain non-ASCII
//! (e.g. `content: "café"`, custom emoji selectors). The matchers are
//! deliberately ASCII-walking, but use char iteration so multi-byte
//! UTF-8 sequences advance correctly. Boundary detection uses Unicode-
//! aware `char::is_whitespace()` to match Rust regex's default `\s`.
//!
//! Property tests fuzz with Unicode haystacks to catch any divergence.

/// A WORD-shape matcher equivalent to the regex
/// `(?i)(^|[\s,(])({NAME}($|[\s(,]))` where NAME is the
/// `regex-escaped` ASCII identifier.
///
/// Used by:
///   - `OldValue.regexp` — `is_match(value)` only
///   - `ValueBase::regexp_cache` — `is_match(value)` AND `replace_all(string, prefix)`
///
/// ## Capture-group preservation for `replace_all`
///
/// Rust regex `replace_all(s, |caps| format!("{}{prefix}{}", caps[1], caps[2]))`
/// expands to: at each match,
///   - `caps[1]` = the boundary char (or empty string if at position 0)
///   - `caps[2]` = NAME followed by the trailing boundary char (or empty)
///   - replacement = `caps[1] + prefix + caps[2]`
///
/// In other words: insert `prefix` between the leading boundary and
/// NAME, leaving everything else unchanged. [`WordMatcher::replace_all_with_prefix`]
/// implements that.
#[cfg_attr(feature = "fast-match", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone)]
pub struct WordMatcher {
    /// The ASCII name (already lowercased so we can compare with
    /// `eq_ignore_ascii_case` on a per-byte basis).
    name_lower: Vec<u8>,
}

impl std::fmt::Debug for WordMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "WordMatcher({})",
            std::str::from_utf8(&self.name_lower).unwrap_or("<non-utf8>")
        )
    }
}

impl WordMatcher {
    /// `name` must be ASCII. The underlying autoprefixer prefix table
    /// is ASCII-only by construction; this is a hard precondition.
    pub fn new(name: &str) -> Self {
        debug_assert!(
            name.is_ascii(),
            "WordMatcher requires ASCII name; got {name:?}"
        );
        let mut buf = name.as_bytes().to_vec();
        buf.make_ascii_lowercase();
        Self { name_lower: buf }
    }

    /// Mirrors `regex::Regex::is_match(haystack)` for the WORD pattern.
    pub fn is_match(&self, haystack: &str) -> bool {
        // Empty NAME would match anywhere — Rust regex treats empty
        // alternation specially; we don't expect it but match by
        // returning false (Rust regex would also do nothing useful).
        if self.name_lower.is_empty() {
            return false;
        }
        find_word(haystack, &self.name_lower).is_some()
    }

    /// Mirrors `regex::Regex::replace_all(haystack, |caps| caps[1] + prefix + caps[2])`.
    /// `caps[1]` = the leading boundary (`""` at pos 0, else 1 char).
    /// `caps[2]` = NAME + trailing boundary char (or end-of-string).
    ///
    /// **Non-overlap semantics (matches Rust regex):** the leading
    /// boundary char AND the trailing boundary char are CONSUMED by
    /// the match — the next scan starts past them. So in
    /// `"flex flex flex"` the first match consumes `"flex "` (pos 0..5),
    /// the middle `"flex"` at pos 5 has no leading boundary available,
    /// only the trailing space is left to anchor the third match.
    /// Result: `"-webkit-flex flex -webkit-flex"` (NOT three replacements).
    pub fn replace_all_with_prefix(&self, haystack: &str, prefix: &str) -> String {
        if self.name_lower.is_empty() {
            return haystack.to_string();
        }
        let mut out = String::with_capacity(haystack.len() + prefix.len() * 2);
        let mut cursor = 0usize;
        for m in find_word_iter(haystack, &self.name_lower) {
            // Verbatim everything before the match.
            out.push_str(&haystack[cursor..m.span_start]);
            // caps[1] — leading boundary char, or empty when at pos 0.
            out.push_str(&haystack[m.span_start..m.name_start]);
            // Inserted prefix.
            out.push_str(prefix);
            // caps[2] — NAME + trailing boundary char (or just NAME at
            // end-of-string).
            out.push_str(&haystack[m.name_start..m.span_end]);
            cursor = m.span_end;
        }
        out.push_str(&haystack[cursor..]);
        out
    }
}

/// One match span produced by [`find_word_iter`] / [`find_selector_iter`].
///
/// `span_*` covers the full regex `caps[0]` extent including consumed
/// boundary chars; `name_*` covers just the NAME portion. Both pairs
/// are byte indices into the original haystack.
#[derive(Debug, Clone, Copy)]
struct Match {
    span_start: usize,
    name_start: usize,
    name_end: usize,
    span_end: usize,
}

/// A SELECTOR-shape matcher equivalent to the regex
/// `(?i)(^|[^:"'=]){NAME}` where NAME is the regex-escaped ASCII
/// selector identifier (or its prefixed form).
///
/// Used by:
///   - `SelectorBase::regexp_cache` — `is_match(selector)` AND
///     `replace_all(selector, |caps| format!("{}{prefixed}", caps[1]))`
///   - `OldSelector.regexp` / `name_regexp` / `prefixeds[*].1` —
///     `is_match` only
#[cfg_attr(feature = "fast-match", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone)]
pub struct SelectorMatcher {
    name_lower: Vec<u8>,
}

impl std::fmt::Debug for SelectorMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SelectorMatcher({})",
            std::str::from_utf8(&self.name_lower).unwrap_or("<non-utf8>")
        )
    }
}

impl SelectorMatcher {
    pub fn new(name: &str) -> Self {
        debug_assert!(
            name.is_ascii(),
            "SelectorMatcher requires ASCII name; got {name:?}"
        );
        let mut buf = name.as_bytes().to_vec();
        buf.make_ascii_lowercase();
        Self { name_lower: buf }
    }

    pub fn is_match(&self, haystack: &str) -> bool {
        if self.name_lower.is_empty() {
            return false;
        }
        find_selector(haystack, &self.name_lower).is_some()
    }

    /// Mirrors `regex.replace_all(haystack, |caps| format!("{}{replacement}", caps[1]))`.
    /// At each match: keep the leading boundary char (caps[1]), drop
    /// NAME, insert `replacement` in NAME's place.
    pub fn replace_all_with(&self, haystack: &str, replacement: &str) -> String {
        if self.name_lower.is_empty() {
            return haystack.to_string();
        }
        let mut out = String::with_capacity(haystack.len() + replacement.len());
        let mut cursor = 0usize;
        for m in find_selector_iter(haystack, &self.name_lower) {
            // Verbatim everything before the match.
            out.push_str(&haystack[cursor..m.span_start]);
            // caps[1] — leading boundary char, or empty when at pos 0.
            out.push_str(&haystack[m.span_start..m.name_start]);
            // NAME is replaced wholesale (selector pattern has no
            // trailing boundary capture).
            out.push_str(replacement);
            cursor = m.span_end;
        }
        out.push_str(&haystack[cursor..]);
        out
    }
}

/// An INTRINSIC-shape matcher equivalent to the regex
/// `(?i)(^|[\s,(])({NAME}($|[\s),]))` where NAME is the regex-escaped
/// ASCII identifier.
///
/// Used by:
///   - `OldValue.regexp` via `OldValueRegexp::Intrinsic` — `is_match`
///     only (`OldValue::check`).
///   - `Intrinsic::regexp_cache` (the hack instance) — `is_match` AND
///     both `replace_all_with_prefix` and `replace_all_with_vendor_alias`
///     (the latter implements the JS `'$1<alias>$3'` substitution where
///     `$3` is the trailing boundary).
///
/// ## Why this isn't `WordMatcher`
///
/// The WORD pattern's trailing class is `[\s(,]`. Intrinsic's is
/// `[\s),]`. The single byte that flips is `(` — WORD admits a `(` as
/// a trailing boundary, Intrinsic does NOT. Inputs like
/// `width: max(fit-content(...))` would silently produce drift if
/// `fit-content` were matched by the WORD pattern: the trailing `(`
/// would be consumed as a boundary char, the matcher would fire, and
/// the output bytes would diverge from the JS oracle.
///
/// `tests/intrinsic_regexp_parity.rs::fit_content_open_paren_must_not_match`
/// pins this single-byte asymmetry as a named test so any drift here
/// surfaces immediately.
#[cfg_attr(feature = "fast-match", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone)]
pub struct IntrinsicMatcher {
    name_lower: Vec<u8>,
}

impl std::fmt::Debug for IntrinsicMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "IntrinsicMatcher({})",
            std::str::from_utf8(&self.name_lower).unwrap_or("<non-utf8>")
        )
    }
}

impl IntrinsicMatcher {
    pub fn new(name: &str) -> Self {
        debug_assert!(
            name.is_ascii(),
            "IntrinsicMatcher requires ASCII name; got {name:?}"
        );
        let mut buf = name.as_bytes().to_vec();
        buf.make_ascii_lowercase();
        Self { name_lower: buf }
    }

    pub fn is_match(&self, haystack: &str) -> bool {
        if self.name_lower.is_empty() {
            return false;
        }
        find_intrinsic(haystack, &self.name_lower).is_some()
    }

    /// Mirrors `regex.replace_all(s, |caps| caps[1] + prefix + caps[2])`.
    /// `caps[1]` = leading boundary char (or empty), `caps[2]` = NAME +
    /// trailing boundary (or just NAME at end-of-string). Identical
    /// shape to `WordMatcher::replace_all_with_prefix`, but using the
    /// Intrinsic trailing class.
    pub fn replace_all_with_prefix(&self, haystack: &str, prefix: &str) -> String {
        if self.name_lower.is_empty() {
            return haystack.to_string();
        }
        let mut out = String::with_capacity(haystack.len() + prefix.len() * 2);
        let mut cursor = 0usize;
        for m in find_intrinsic_iter(haystack, &self.name_lower) {
            out.push_str(&haystack[cursor..m.span_start]);
            // caps[1]
            out.push_str(&haystack[m.span_start..m.name_start]);
            // inserted prefix
            out.push_str(prefix);
            // caps[2] = NAME + trailing-or-eos
            out.push_str(&haystack[m.name_start..m.span_end]);
            cursor = m.span_end;
        }
        out.push_str(&haystack[cursor..]);
        out
    }

    /// Mirrors `regex.replace_all(s, |caps| caps[1] + alias + caps[3])`
    /// — the JS-side `Intrinsic::replace` stretch-family branch. `caps[1]`
    /// = leading boundary, `caps[3]` = trailing boundary alone (NOT
    /// including NAME). NAME is dropped, replaced by `alias`.
    pub fn replace_all_with_vendor_alias(
        &self,
        haystack: &str,
        alias: &str,
    ) -> String {
        if self.name_lower.is_empty() {
            return haystack.to_string();
        }
        let mut out = String::with_capacity(haystack.len() + alias.len());
        let mut cursor = 0usize;
        for m in find_intrinsic_iter(haystack, &self.name_lower) {
            out.push_str(&haystack[cursor..m.span_start]);
            // caps[1]
            out.push_str(&haystack[m.span_start..m.name_start]);
            // alias replaces NAME
            out.push_str(alias);
            // caps[3] = trailing boundary char (or empty at end-of-string).
            // span_end - name_end is 0 at EOS, 1 char otherwise.
            out.push_str(&haystack[m.name_end..m.span_end]);
            cursor = m.span_end;
        }
        out.push_str(&haystack[cursor..]);
        out
    }
}

// --------------------------------------------------------------------------
// Internal scanners
// --------------------------------------------------------------------------

/// Match leading boundary class for the WORD pattern: position 0, OR
/// previous char in `[\s,(]` (Unicode whitespace, comma, open-paren).
#[inline]
fn is_word_left_boundary(c: char) -> bool {
    c.is_whitespace() || c == ',' || c == '('
}

/// Match trailing boundary class for the WORD pattern: end of string,
/// OR next char in `[\s(,]` (Unicode whitespace, open-paren, comma).
#[inline]
fn is_word_right_boundary(c: char) -> bool {
    c.is_whitespace() || c == ',' || c == '('
}

/// Match leading boundary class for the INTRINSIC pattern: identical to
/// WORD's left class (`[\s,(]`). Both patterns share `(^|[\s,(])` as
/// their leading group — the only difference between WORD and INTRINSIC
/// is the trailing class.
#[inline]
fn is_intrinsic_left_boundary(c: char) -> bool {
    c.is_whitespace() || c == ',' || c == '('
}

/// Match trailing boundary class for the INTRINSIC pattern: end of
/// string, OR next char in `[\s),]` (Unicode whitespace, close-paren,
/// comma). The single byte that differs from WORD is `(` — WORD admits
/// it as a trailing boundary, INTRINSIC does NOT. That asymmetry is
/// the entire reason `IntrinsicMatcher` exists; without it,
/// `width: max(fit-content(...))`-shaped values would produce drift
/// because WORD would erroneously match `fit-content(`. See
/// `tests/intrinsic_regexp_parity.rs::fit_content_open_paren_must_not_match`.
#[inline]
fn is_intrinsic_right_boundary(c: char) -> bool {
    c.is_whitespace() || c == ')' || c == ','
}

/// Match leading boundary class for the SELECTOR pattern: position 0,
/// OR previous char NOT in `[:"'=]`.
#[inline]
fn is_selector_left_boundary(c: char) -> bool {
    c != ':' && c != '"' && c != '\'' && c != '='
}

/// Find first WORD-boundary match in `haystack`. Used by `is_match`.
/// Returns the full `Match` (with span) but `is_match` only checks
/// `Option::is_some`.
fn find_word(haystack: &str, needle_lower: &[u8]) -> Option<Match> {
    find_word_iter(haystack, needle_lower).next()
}

/// Iterator over non-overlapping WORD-boundary matches.
///
/// Matches Rust regex `replace_all` semantics: each yielded match
/// CONSUMES the leading and trailing boundary chars (when present),
/// so subsequent matches cannot reuse them. This is critical for
/// `replace_all` to produce regex-equivalent bytes — see
/// `tests/fast_match_parity.rs::word_matcher_corners` for the
/// `"flex flex flex"` test that exercises this.
fn find_word_iter<'a>(
    haystack: &'a str,
    needle_lower: &'a [u8],
) -> WordIter<'a> {
    WordIter {
        haystack,
        needle: needle_lower,
        cursor: 0,
    }
}

struct WordIter<'a> {
    haystack: &'a str,
    needle: &'a [u8],
    cursor: usize,
}

impl<'a> Iterator for WordIter<'a> {
    type Item = Match;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.haystack.as_bytes();
        let n = self.needle.len();
        let total = bytes.len();
        if n == 0 || self.cursor + n > total {
            return None;
        }
        let mut name_start = self.cursor;
        while name_start + n <= total {
            // name_start must lie on a UTF-8 char boundary.
            if !self.haystack.is_char_boundary(name_start) {
                name_start += 1;
                continue;
            }

            // Left-boundary detection. If at the cursor's start position
            // (NOT the haystack's start — Rust regex non-overlapping
            // semantics: a previously-consumed boundary char no longer
            // counts), `^` doesn't match, but a boundary char in the
            // un-consumed remainder MIGHT.
            //
            // The regex `(^|[\s,(])` captures EITHER nothing (zero-width
            // ^ at haystack pos 0 — only at the very first scan) OR a
            // single boundary char. After replace_all consumes a match
            // including the leading boundary char, the regex resumes at
            // the consumed-end position; ^ no longer matches there
            // because the regex's anchor is the start-of-haystack,
            // not start-of-remaining.
            //
            // We model this by tracking `cursor` = position past the
            // last consumed match. When `cursor == 0`, `^` is available;
            // otherwise it isn't. The leading-boundary CHAR alternative
            // requires `name_start > cursor` so the boundary char fits
            // BEFORE NAME and after the consumed prefix.
            let (span_start, leading_consumed) = if name_start == 0 {
                // ^ alternative — caps[1] is empty.
                (0usize, false)
            } else if name_start > self.cursor {
                // Leading boundary char alternative — caps[1] is 1 char
                // sitting BEFORE NAME and AFTER the consumed prefix.
                let bc_end = name_start;
                let bc_start = prev_char_start(self.haystack, bc_end);
                if bc_start < self.cursor {
                    // Boundary char overlaps the consumed prefix — not
                    // available.
                    name_start += 1;
                    continue;
                }
                let prev = self.haystack[bc_start..bc_end]
                    .chars()
                    .next()
                    .expect("non-empty char span");
                if !is_word_left_boundary(prev) {
                    name_start += 1;
                    continue;
                }
                (bc_start, true)
            } else {
                // name_start == self.cursor (and != 0). No room for a
                // leading boundary char before NAME, and ^ doesn't
                // match at non-zero positions. No match here.
                name_start += 1;
                continue;
            };

            // NAME match: ASCII case-insensitive byte compare.
            if !ascii_eq_ignore_case(&bytes[name_start..name_start + n], self.needle)
            {
                // Cheap exit: bump by 1 byte and re-anchor the boundary
                // search. (UTF-8 boundary check at top of loop covers
                // mid-codepoint positions.)
                name_start += 1;
                continue;
            }

            let name_end = name_start + n;

            // Trailing boundary: at end-of-haystack OR next char is
            // whitespace/`(`/`,`. The trailing char (when present) is
            // CONSUMED — span_end advances past it.
            let span_end = if name_end == total {
                name_end
            } else if !self.haystack.is_char_boundary(name_end) {
                // Mid-codepoint trailing — match invalid.
                name_start += 1;
                continue;
            } else {
                let next_start = name_end;
                let next_end = next_char_end(self.haystack, next_start);
                let next = self.haystack[next_start..next_end]
                    .chars()
                    .next()
                    .expect("non-empty char span");
                if !is_word_right_boundary(next) {
                    name_start += 1;
                    continue;
                }
                next_end
            };

            let _ = leading_consumed; // documentation only
            self.cursor = span_end;
            return Some(Match {
                span_start,
                name_start,
                name_end,
                span_end,
            });
        }
        self.cursor = total;
        None
    }
}

/// Find first INTRINSIC-boundary match.
fn find_intrinsic(haystack: &str, needle_lower: &[u8]) -> Option<Match> {
    find_intrinsic_iter(haystack, needle_lower).next()
}

/// Iterator over non-overlapping INTRINSIC-boundary matches. Mirrors
/// `WordIter` byte-for-byte except for the right-boundary predicate
/// — INTRINSIC uses `[\s),]` (close-paren), WORD uses `[\s(,]`
/// (open-paren). The single-byte asymmetry on `(` is the entire reason
/// this iterator exists separately from `WordIter`.
fn find_intrinsic_iter<'a>(
    haystack: &'a str,
    needle_lower: &'a [u8],
) -> IntrinsicIter<'a> {
    IntrinsicIter {
        haystack,
        needle: needle_lower,
        cursor: 0,
    }
}

struct IntrinsicIter<'a> {
    haystack: &'a str,
    needle: &'a [u8],
    cursor: usize,
}

impl<'a> Iterator for IntrinsicIter<'a> {
    type Item = Match;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.haystack.as_bytes();
        let n = self.needle.len();
        let total = bytes.len();
        if n == 0 || self.cursor + n > total {
            return None;
        }
        let mut name_start = self.cursor;
        while name_start + n <= total {
            if !self.haystack.is_char_boundary(name_start) {
                name_start += 1;
                continue;
            }

            // Left-boundary detection — identical to WordIter (same class).
            let span_start = if name_start == 0 {
                0usize
            } else if name_start > self.cursor {
                let bc_end = name_start;
                let bc_start = prev_char_start(self.haystack, bc_end);
                if bc_start < self.cursor {
                    name_start += 1;
                    continue;
                }
                let prev = self.haystack[bc_start..bc_end]
                    .chars()
                    .next()
                    .expect("non-empty char span");
                if !is_intrinsic_left_boundary(prev) {
                    name_start += 1;
                    continue;
                }
                bc_start
            } else {
                name_start += 1;
                continue;
            };

            // NAME match: ASCII case-insensitive byte compare.
            if !ascii_eq_ignore_case(&bytes[name_start..name_start + n], self.needle)
            {
                name_start += 1;
                continue;
            }

            let name_end = name_start + n;

            // Trailing boundary — INTRINSIC class `[\s),]`. The single
            // byte that flips from WORD is `(` — INTRINSIC rejects it.
            let span_end = if name_end == total {
                name_end
            } else if !self.haystack.is_char_boundary(name_end) {
                name_start += 1;
                continue;
            } else {
                let next_start = name_end;
                let next_end = next_char_end(self.haystack, next_start);
                let next = self.haystack[next_start..next_end]
                    .chars()
                    .next()
                    .expect("non-empty char span");
                if !is_intrinsic_right_boundary(next) {
                    name_start += 1;
                    continue;
                }
                next_end
            };

            self.cursor = span_end;
            return Some(Match {
                span_start,
                name_start,
                name_end,
                span_end,
            });
        }
        self.cursor = total;
        None
    }
}

/// Find first SELECTOR-boundary match.
fn find_selector(haystack: &str, needle_lower: &[u8]) -> Option<Match> {
    find_selector_iter(haystack, needle_lower).next()
}

fn find_selector_iter<'a>(
    haystack: &'a str,
    needle_lower: &'a [u8],
) -> SelectorIter<'a> {
    SelectorIter {
        haystack,
        needle: needle_lower,
        cursor: 0,
    }
}

struct SelectorIter<'a> {
    haystack: &'a str,
    needle: &'a [u8],
    cursor: usize,
}

impl<'a> Iterator for SelectorIter<'a> {
    type Item = Match;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.haystack.as_bytes();
        let n = self.needle.len();
        let total = bytes.len();
        if n == 0 || self.cursor + n > total {
            return None;
        }
        let mut name_start = self.cursor;
        while name_start + n <= total {
            if !self.haystack.is_char_boundary(name_start) {
                name_start += 1;
                continue;
            }

            // Selector pattern: `(^|[^:"'=])NAME`. Same cursor-aware
            // boundary logic as WordIter.
            let span_start = if name_start == 0 {
                0
            } else if name_start > self.cursor {
                let bc_end = name_start;
                let bc_start = prev_char_start(self.haystack, bc_end);
                if bc_start < self.cursor {
                    name_start += 1;
                    continue;
                }
                let prev = self.haystack[bc_start..bc_end]
                    .chars()
                    .next()
                    .expect("non-empty char span");
                if !is_selector_left_boundary(prev) {
                    name_start += 1;
                    continue;
                }
                bc_start
            } else {
                name_start += 1;
                continue;
            };

            if !ascii_eq_ignore_case(&bytes[name_start..name_start + n], self.needle)
            {
                name_start += 1;
                continue;
            }

            let name_end = name_start + n;
            self.cursor = name_end;
            return Some(Match {
                span_start,
                name_start,
                name_end,
                span_end: name_end,
            });
        }
        self.cursor = total;
        None
    }
}

/// Byte index where the char ending exactly at `byte_end` STARTS.
/// Walks back from `byte_end - 1` until a UTF-8 leading byte is found.
#[inline]
fn prev_char_start(s: &str, byte_end: usize) -> usize {
    debug_assert!(byte_end > 0);
    debug_assert!(s.is_char_boundary(byte_end));
    let bytes = s.as_bytes();
    let mut i = byte_end - 1;
    while i > 0 && (bytes[i] & 0b1100_0000) == 0b1000_0000 {
        i -= 1;
    }
    i
}

/// Byte index where the char starting at `byte_start` ENDS.
#[inline]
fn next_char_end(s: &str, byte_start: usize) -> usize {
    debug_assert!(s.is_char_boundary(byte_start));
    debug_assert!(byte_start < s.len());
    let bytes = s.as_bytes();
    let lead = bytes[byte_start];
    let len = if lead < 0x80 {
        1
    } else if lead < 0xC0 {
        // Continuation byte — shouldn't be hit because of the
        // `is_char_boundary` precondition, but be defensive.
        1
    } else if lead < 0xE0 {
        2
    } else if lead < 0xF0 {
        3
    } else {
        4
    };
    byte_start + len
}

/// ASCII case-insensitive byte slice equality. Returns false if any
/// byte in `a` is non-ASCII OR doesn't ascii-case-match the
/// corresponding byte in `b_lowered`.
#[inline]
fn ascii_eq_ignore_case(a: &[u8], b_lowered: &[u8]) -> bool {
    if a.len() != b_lowered.len() {
        return false;
    }
    for (&x, &y) in a.iter().zip(b_lowered.iter()) {
        // y is already lowercase. x may be any byte; if non-ASCII or
        // non-letter, ASCII-lowercase is a no-op so direct compare
        // works. For ASCII letters, lowercase before compare.
        let x_low = x.to_ascii_lowercase();
        if x_low != y {
            return false;
        }
    }
    true
}

// --------------------------------------------------------------------------
// Public wrapper types — feature-gated swap-out for `regex::Regex`
// --------------------------------------------------------------------------
//
// `WordRegexp` and `SelectorRegexp` are the feature-flag swap layer.
// With `--features fast-match`, the wrappers carry our hand-rolled
// matchers and avoid regex compilation entirely. Without the feature,
// they wrap a `regex::Regex` compiled with the same pattern the
// original code paths used — preserving the slow path verbatim for
// instant revert if drift is ever found in production.
//
// **Drift contract:** the WITH-feature path's bytes-out MUST equal the
// WITHOUT-feature path's bytes-out for every input. Property tests in
// `tests/fast_match_parity.rs` lock the matchers down; the
// parity-runner 337-fixture corpus is the integration gate.

/// Word-pattern wrapper — equivalent to `regex::Regex` compiled from
/// `(?i)(^|[\s,(])({}($|[\s(,]))` where `{}` is the regex-escaped name.
///
/// Hot-path used by `OldValue.regexp` and `ValueBase`'s lazy regexp
/// cache. Construction cost is what we're attacking: with `fast-match`
/// the wrapper is a `WordMatcher` (no regex compile); without it, we
/// fall back to the original regex compile path (slow but parity-safe).
#[cfg_attr(feature = "fast-match", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct WordRegexp {
    #[cfg(feature = "fast-match")]
    inner: WordMatcher,
    #[cfg(not(feature = "fast-match"))]
    inner: regex::Regex,
}

impl WordRegexp {
    /// `name` is the unescaped identifier (e.g. `"flex"`,
    /// `"-webkit-flex"`, `"linear-gradient"`). The regex-side branch
    /// performs the same `escape_regexp + format` the original
    /// `utils::regexp` did, routed through the profiling counter.
    pub fn new(name: &str) -> Self {
        #[cfg(feature = "fast-match")]
        {
            Self { inner: WordMatcher::new(name) }
        }
        #[cfg(not(feature = "fast-match"))]
        {
            let escaped = crate::utils::escape_regexp(name);
            let pat = format!(r"(?i)(^|[\s,(])({}($|[\s(,]))", escaped);
            let re = crate::profile::time_regex_compile(|| {
                regex::Regex::new(&pat).expect("valid word regex")
            });
            Self { inner: re }
        }
    }

    pub fn is_match(&self, haystack: &str) -> bool {
        self.inner.is_match(haystack)
    }

    /// Mirrors `re.replace_all(s, |caps| caps[1] + prefix + caps[2])`
    /// from `value.rs` and the JS oracle. Returns owned String (the
    /// regex path's `Cow` is only borrowed when no match occurred —
    /// flattening to String costs nothing in the no-match case
    /// because postcard parity tests confirm byte equality).
    pub fn replace_all_with_prefix(&self, haystack: &str, prefix: &str) -> String {
        #[cfg(feature = "fast-match")]
        {
            self.inner.replace_all_with_prefix(haystack, prefix)
        }
        #[cfg(not(feature = "fast-match"))]
        {
            self.inner
                .replace_all(haystack, |caps: &regex::Captures| {
                    format!(
                        "{}{}{}",
                        caps.get(1).map(|m| m.as_str()).unwrap_or(""),
                        prefix,
                        caps.get(2).map(|m| m.as_str()).unwrap_or(""),
                    )
                })
                .into_owned()
        }
    }
}

/// Selector-pattern wrapper — equivalent to `regex::Regex` compiled
/// from `(?i)(^|[^:"'=]){}` where `{}` is the regex-escaped selector
/// name (or its prefixed form).
///
/// Hot-path used by `SelectorBase`'s regexp cache, `OldSelector` /
/// `SelectorView` fields, and the prefixed-form check loop in
/// `OldSelector::is_hack`.
#[cfg_attr(feature = "fast-match", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct SelectorRegexp {
    #[cfg(feature = "fast-match")]
    inner: SelectorMatcher,
    #[cfg(not(feature = "fast-match"))]
    inner: regex::Regex,
}

impl SelectorRegexp {
    pub fn new(name: &str) -> Self {
        #[cfg(feature = "fast-match")]
        {
            Self { inner: SelectorMatcher::new(name) }
        }
        #[cfg(not(feature = "fast-match"))]
        {
            let escaped = crate::utils::escape_regexp(name);
            let pat = format!(r#"(?i)(^|[^:"'=]){}"#, escaped);
            let re = crate::profile::time_regex_compile(|| {
                regex::Regex::new(&pat).expect("valid selector regex")
            });
            Self { inner: re }
        }
    }

    pub fn is_match(&self, haystack: &str) -> bool {
        self.inner.is_match(haystack)
    }

    /// Mirrors `re.replace_all(s, |caps| format!("{}{}", caps[1], replacement))`
    /// from `selector.rs::replace`.
    pub fn replace_all_with(&self, haystack: &str, replacement: &str) -> String {
        #[cfg(feature = "fast-match")]
        {
            self.inner.replace_all_with(haystack, replacement)
        }
        #[cfg(not(feature = "fast-match"))]
        {
            self.inner
                .replace_all(haystack, |caps: &regex::Captures| {
                    format!(
                        "{}{}",
                        caps.get(1).map(|m| m.as_str()).unwrap_or(""),
                        replacement,
                    )
                })
                .into_owned()
        }
    }
}

/// Intrinsic-pattern wrapper — equivalent to `regex::Regex` compiled
/// from `(?i)(^|[\s,(])({}($|[\s),]))` where `{}` is the regex-escaped
/// name.
///
/// Hot-path used by the `OldValueRegexp::Intrinsic` variant (consumed
/// by `OldValue::check`) and by the `Intrinsic` hack's lazy regexp
/// cache (consumed by `ValuePrefixer::check` and the hack's `replace`
/// implementation).
///
/// **Drift contract:** byte-equal to the equivalent `regex::Regex` for
/// every input. The single-byte trailing-class asymmetry vs WORD is
/// load-bearing — see `IntrinsicMatcher` doc + `tests/intrinsic_regexp_parity.rs`.
#[cfg(feature = "fast-match")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IntrinsicRegexp {
    inner: IntrinsicMatcher,
}

#[cfg(not(feature = "fast-match"))]
#[derive(Debug, Clone)]
pub struct IntrinsicRegexp {
    inner: regex::Regex,
    /// Original name — preserved so `replace_all_with_prefix` and
    /// `replace_all_with_vendor_alias` can rebuild the same capture
    /// access pattern the JS oracle uses.
    #[allow(dead_code)]
    name: String,
}

impl IntrinsicRegexp {
    pub fn new(name: &str) -> Self {
        #[cfg(feature = "fast-match")]
        {
            Self { inner: IntrinsicMatcher::new(name) }
        }
        #[cfg(not(feature = "fast-match"))]
        {
            let escaped = crate::utils::escape_regexp(name);
            let pat =
                format!(r"(?i)(^|[\s,(])({}($|[\s),]))", escaped);
            let re = crate::profile::time_regex_compile(|| {
                regex::Regex::new(&pat).expect("valid intrinsic regex")
            });
            Self { inner: re, name: name.to_string() }
        }
    }

    pub fn is_match(&self, haystack: &str) -> bool {
        self.inner.is_match(haystack)
    }

    /// `caps[1] + prefix + caps[2]` — JS Intrinsic.replace non-stretch.
    pub fn replace_all_with_prefix(&self, haystack: &str, prefix: &str) -> String {
        #[cfg(feature = "fast-match")]
        {
            self.inner.replace_all_with_prefix(haystack, prefix)
        }
        #[cfg(not(feature = "fast-match"))]
        {
            self.inner
                .replace_all(haystack, |caps: &regex::Captures| {
                    format!(
                        "{}{}{}",
                        caps.get(1).map(|m| m.as_str()).unwrap_or(""),
                        prefix,
                        caps.get(2).map(|m| m.as_str()).unwrap_or(""),
                    )
                })
                .into_owned()
        }
    }

    /// `caps[1] + alias + caps[3]` — JS Intrinsic.replace stretch-family
    /// branch (drops NAME, inserts vendor alias between leading and
    /// trailing boundaries).
    pub fn replace_all_with_vendor_alias(
        &self,
        haystack: &str,
        alias: &str,
    ) -> String {
        #[cfg(feature = "fast-match")]
        {
            self.inner.replace_all_with_vendor_alias(haystack, alias)
        }
        #[cfg(not(feature = "fast-match"))]
        {
            self.inner
                .replace_all(haystack, |caps: &regex::Captures| {
                    format!(
                        "{}{}{}",
                        caps.get(1).map(|m| m.as_str()).unwrap_or(""),
                        alias,
                        caps.get(3).map(|m| m.as_str()).unwrap_or(""),
                    )
                })
                .into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests live here for the cheap cases. The expensive parity
    //! tests (fuzz vs Rust regex) live in
    //! `tests/fast_match_parity.rs`.

    use super::*;

    #[test]
    fn word_matcher_simple_match() {
        let m = WordMatcher::new("flex");
        assert!(m.is_match("display: flex"));
        assert!(m.is_match("flex"));
        assert!(m.is_match("(flex,"));         // trailing ',' IS in [\s(,]
        assert!(m.is_match("flex,wrap"));
        assert!(!m.is_match("(flex)"));        // trailing ')' NOT in [\s(,]
        assert!(!m.is_match("flexbox"));
        assert!(!m.is_match("display:flex"));  // leading ':' NOT in [\s,(]
    }

    #[test]
    fn word_matcher_case_insensitive() {
        let m = WordMatcher::new("flex");
        assert!(m.is_match("FLEX"));
        assert!(m.is_match("Flex"));
        assert!(m.is_match("display: FLEX"));
    }

    #[test]
    fn word_matcher_replace_inserts_prefix() {
        let m = WordMatcher::new("flex");
        assert_eq!(
            m.replace_all_with_prefix("display: flex", "-webkit-"),
            "display: -webkit-flex"
        );
        assert_eq!(
            m.replace_all_with_prefix("flex", "-webkit-"),
            "-webkit-flex"
        );
        // Non-overlapping consumption: first match consumes `(flex,`
        // (leading `(` and trailing `,` are CONSUMED by `caps[0]` per
        // regex semantics). Cursor advances to pos 6; the second `flex`
        // at pos 6 has no remaining leading-boundary char before it
        // (the `,` at pos 5 was consumed) and `^` doesn't anchor at
        // non-zero positions, so no second match. Output: only the
        // first `flex` gets prefixed.
        //
        // Verified byte-equal to JS `'(flex,flex)'.replace(re, '$1-webkit-$2')`
        // and Rust `regex::Regex::replace_all`.
        assert_eq!(
            m.replace_all_with_prefix("(flex,flex)", "-webkit-"),
            "(-webkit-flex,flex)"
        );
    }

    #[test]
    fn word_matcher_no_match_returns_haystack_unchanged() {
        let m = WordMatcher::new("flex");
        assert_eq!(m.replace_all_with_prefix("none", "-webkit-"), "none");
    }

    #[test]
    fn selector_matcher_basic() {
        let m = SelectorMatcher::new(":fullscreen");
        assert!(m.is_match(":fullscreen"));
        assert!(m.is_match("a :fullscreen"));
        assert!(!m.is_match("::fullscreen")); // ':' is in NOT-allowed set
        assert!(!m.is_match("='fullscreen'"));
    }

    #[test]
    fn selector_matcher_replace() {
        let m = SelectorMatcher::new(":fullscreen");
        assert_eq!(
            m.replace_all_with(":fullscreen", ":-webkit-fullscreen"),
            ":-webkit-fullscreen"
        );
        assert_eq!(
            m.replace_all_with("a :fullscreen", ":-webkit-fullscreen"),
            "a :-webkit-fullscreen"
        );
    }

    #[test]
    fn unicode_haystack_doesnt_falsely_match() {
        let m = WordMatcher::new("flex");
        // Non-ASCII chars before/after must NOT count as boundaries
        // unless they're whitespace.
        assert!(!m.is_match("xflex"));
        assert!(!m.is_match("café-flex"));
        // But Unicode whitespace IS in the boundary class (matches
        // Rust regex `\s` default).
        assert!(m.is_match("café \u{2003}flex")); // U+2003 EM SPACE (whitespace)
    }
}
