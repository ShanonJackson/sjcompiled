//! Port of `postcss-selector-parser/dist/util/*.js`.
//! - `stripComments.js` -> [`strip_comments`]
//! - `unesc.js`         -> [`unesc`]
//! - `getProp.js`       -> [`get_prop`]

/// Port of `util/stripComments.js`.
pub fn strip_comments(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(cc) = chars.next() {
                if cc == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Port of `util/unesc.js`. CSS escape unescape per the upstream regex
/// `(\\(?:([0-9a-f]{1,6}\s?|.)|\\$|$))/`.
/// NOTE: scaffold — full table-driven port pending.
pub fn unesc(s: &str) -> String { s.to_string() }

/// Port of `util/getProp.js` — returns nested property by string path.
/// Provided as a generic placeholder; real callers go through the AST.
pub fn get_prop<'a>(_obj: &'a str, _path: &[&str]) -> Option<&'a str> { None }
