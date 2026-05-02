//! Port of Node `lib/path.js` (POSIX subset only — `path.posix.normalize`).
//!
//! Upstream `postcss-normalize-url@5.1.0` calls `require('path').normalize(url)`
//! which is the OS-active `path` module (POSIX on Linux/macOS, Win32 on
//! Windows). Production builds of the consuming monorepo run on Linux, so
//! this port targets `path.posix.normalize` semantics. On Windows hosts
//! the JS oracle would diverge from this port for inputs containing
//! backslashes or Windows drive letters — those inputs are unlikely in CSS
//! `url(...)` calls (which use forward-slash URL syntax, not file paths).
//!
//! Algorithm mirrors Node v16+ `lib/path.js` `posix.normalize` byte-for-byte:
//!
//! ```js
//! normalize(path) {
//!   if (path.length === 0) return '.';
//!   const isAbsolute = path.charCodeAt(0) === CHAR_FORWARD_SLASH;
//!   const trailingSeparator = path.charCodeAt(path.length - 1) === CHAR_FORWARD_SLASH;
//!   path = normalizeString(path, !isAbsolute, '/', isPosixPathSeparator);
//!   if (path.length === 0) {
//!     if (isAbsolute) return '/';
//!     return trailingSeparator ? './' : '.';
//!   }
//!   if (trailingSeparator) path += '/';
//!   return isAbsolute ? `/${path}` : path;
//! }
//! ```

/// Mirrors Node's `posix.normalize(path)`.
pub fn posix_normalize(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }

    let bytes = path.as_bytes();
    let is_absolute = bytes[0] == b'/';
    let trailing_separator = bytes[bytes.len() - 1] == b'/';

    let mut normalized = normalize_string(path, !is_absolute, b'/');

    if normalized.is_empty() {
        if is_absolute {
            return "/".to_string();
        }
        return if trailing_separator { "./".to_string() } else { ".".to_string() };
    }

    if trailing_separator {
        normalized.push('/');
    }

    if is_absolute {
        format!("/{normalized}")
    } else {
        normalized
    }
}

/// Mirrors Node's internal `normalizeString(path, allowAboveRoot, separator, isPathSeparator)`.
/// Posix-only: separator is always `'/'`.
fn normalize_string(path: &str, allow_above_root: bool, separator: u8) -> String {
    let bytes = path.as_bytes();
    let mut res = String::new();
    let mut last_segment_length: usize = 0;
    // Position of the LAST separator written into `res`. -1 sentinel via Option.
    let mut last_slash: Option<usize> = None;
    let mut dots: i32 = 0;

    let mut i: usize = 0;
    while i <= bytes.len() {
        let code: i32 = if i < bytes.len() {
            bytes[i] as i32
        } else if i == bytes.len() {
            // Sentinel: when i == bytes.len(), upstream sets `code = path.charCodeAt(i)`
            // which is NaN; the comparison `if (...)` then takes the separator branch.
            separator as i32
        } else {
            break;
        };

        let is_separator = code == separator as i32;
        // Upstream: `if (isPathSeparator(code))` — true when at the sentinel
        // i==len position too because we treat it as a separator.
        if is_separator || i == bytes.len() {
            // Upstream: `if (lastSlash === i - 1 || dots === 1)` — empty
            // segment or single-dot segment: skip.
            let prev_was_slash = match last_slash {
                Some(p) => p + 1 == i,
                None => i == 0,
            };
            if prev_was_slash || dots == 1 {
                // Skip this segment.
            } else if dots == 2 {
                // `..` segment.
                let res_bytes = res.as_bytes();
                let need_pop = res.len() < 2
                    || last_segment_length != 2
                    || res_bytes[res.len() - 1] != b'.'
                    || res_bytes[res.len() - 2] != b'.';
                if need_pop {
                    if res.len() > 2 {
                        // Drop the last segment in `res`.
                        let last_slash_index = res.as_bytes().iter().rposition(|&b| b == separator);
                        match last_slash_index {
                            Some(idx) => {
                                res.truncate(idx);
                                last_segment_length = match res.as_bytes().iter().rposition(|&b| b == separator) {
                                    Some(p) => res.len() - 1 - p,
                                    None => res.len(),
                                };
                            }
                            None => {
                                res.clear();
                                last_segment_length = 0;
                            }
                        }
                        last_slash = Some(i);
                        dots = 0;
                        i += 1;
                        continue;
                    } else if !res.is_empty() {
                        res.clear();
                        last_segment_length = 0;
                        last_slash = Some(i);
                        dots = 0;
                        i += 1;
                        continue;
                    }
                }
                if allow_above_root {
                    if !res.is_empty() {
                        res.push_str("/..");
                    } else {
                        res.push_str("..");
                    }
                    last_segment_length = 2;
                }
            } else {
                // Real segment.
                let segment_start = match last_slash {
                    Some(p) => p + 1,
                    None => 0,
                };
                let segment = &path[segment_start..i];
                if !res.is_empty() {
                    res.push(separator as char);
                    res.push_str(segment);
                } else {
                    res.push_str(segment);
                }
                last_segment_length = segment.len();
            }
            last_slash = Some(i);
            dots = 0;
        } else if code == b'.' as i32 && dots != -1 {
            dots += 1;
        } else {
            dots = -1;
        }
        i += 1;
    }

    res
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vectors from Node REPL @ v18.x:
    //   require('path').posix.normalize(<input>)
    #[test]
    fn empty() { assert_eq!(posix_normalize(""), "."); }
    #[test]
    fn dot() { assert_eq!(posix_normalize("."), "."); }
    #[test]
    fn dot_dot() { assert_eq!(posix_normalize(".."), ".."); }
    #[test]
    fn root() { assert_eq!(posix_normalize("/"), "/"); }
    #[test]
    fn relative_simple() { assert_eq!(posix_normalize("foo/bar"), "foo/bar"); }
    #[test]
    fn relative_dot_prefix() { assert_eq!(posix_normalize("./foo/bar"), "foo/bar"); }
    #[test]
    fn relative_collapse_dotdot() { assert_eq!(posix_normalize("foo/../bar"), "bar"); }
    #[test]
    fn relative_dotdot_above() { assert_eq!(posix_normalize("../foo"), "../foo"); }
    #[test]
    fn duplicate_slashes() { assert_eq!(posix_normalize("foo//bar"), "foo/bar"); }
    #[test]
    fn trailing_slash() { assert_eq!(posix_normalize("foo/bar/"), "foo/bar/"); }
    #[test]
    fn absolute_with_dotdot() { assert_eq!(posix_normalize("/foo/bar/../baz"), "/foo/baz"); }
    #[test]
    fn absolute_collapses() { assert_eq!(posix_normalize("/../foo"), "/foo"); }
    #[test]
    fn complex_a() { assert_eq!(posix_normalize("/foo/bar//baz/asdf/quux/.."), "/foo/bar/baz/asdf"); }
    #[test]
    fn complex_b() { assert_eq!(posix_normalize("a/./b/c/.."), "a/b"); }
    #[test]
    fn dot_only() { assert_eq!(posix_normalize("./"), "./"); }
}
