//! Port of `postcss/lib/list.js`.

/// `list.split(string, separators, last)` — upstream allows multiple separator
/// chars and respects parenthesis/quote nesting.
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
            if !current.is_empty() { array.push(std::mem::take(&mut current)); }
            split_now = false;
        } else {
            current.push(ch);
        }
    }
    if last || !current.is_empty() { array.push(current); }
    array
}

pub fn space(input: &str) -> Vec<String> {
    let seps = [' ', '\n', '\t'];
    split(input, &seps, false)
}

pub fn comma(input: &str) -> Vec<String> {
    split(input, &[','], true)
}
