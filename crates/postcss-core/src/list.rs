//! Port of `postcss/lib/list.js`.

/// `list.split(string, separators, last)` — upstream allows multiple separator
/// chars and respects parenthesis/quote nesting. Pushed values are
/// `String.prototype.trim()`-equivalent (matches upstream which does
/// `array.push(current.trim())` on each segment).
pub fn split(input: &str, separators: &[char], last: bool) -> Vec<String> {
    let mut array: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut split_now = false;
    let mut func: i32 = 0;
    let mut quote = false;
    let mut escape = false;
    let mut last_char: Option<char> = None;

    for ch in input.chars() {
        if escape {
            escape = false;
        } else if ch == '\\' {
            escape = true;
        } else if quote {
            if ch == last_char.unwrap_or('\0') {
                quote = false;
            }
        } else if ch == '"' || ch == '\'' {
            quote = true;
            last_char = Some(ch);
        } else if ch == '(' { func += 1; }
        else if ch == ')' { if func > 0 { func -= 1; } }
        else if func == 0 && separators.contains(&ch) {
            split_now = true;
        }
        if split_now {
            if !current.is_empty() {
                array.push(std::mem::take(&mut current).trim().to_string());
            }
            split_now = false;
        } else {
            current.push(ch);
        }
    }
    if last || !current.is_empty() {
        array.push(current.trim().to_string());
    }
    array
}

pub fn space(input: &str) -> Vec<String> {
    let seps = [' ', '\n', '\t'];
    split(input, &seps, false)
}

pub fn comma(input: &str) -> Vec<String> {
    split(input, &[','], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comma_trims_each() {
        assert_eq!(comma(".a, .b, .c"), vec![".a", ".b", ".c"]);
        assert_eq!(comma(".a,.b"), vec![".a", ".b"]);
        assert_eq!(comma(".a,\n.b"), vec![".a", ".b"]);
    }

    #[test]
    fn comma_respects_parens() {
        assert_eq!(comma(":is(.a, .b), .c"), vec![":is(.a, .b)", ".c"]);
    }

    #[test]
    fn comma_respects_quotes() {
        assert_eq!(comma(r#"[data-x=","]"#), vec![r#"[data-x=","]"#]);
    }

    #[test]
    fn comma_empty_input() {
        // last=true on `comma`, so an empty trailing segment IS pushed.
        assert_eq!(comma(""), vec![""]);
    }

    #[test]
    fn space_does_not_emit_trailing_empty() {
        // last=false on `space`, so no trailing empty.
        assert_eq!(space("a b c"), vec!["a", "b", "c"]);
    }
}
