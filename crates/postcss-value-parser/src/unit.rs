//! Port of `postcss-value-parser/lib/unit.js`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUnit {
    pub number: String,
    pub unit: String,
}

const MINUS: u8 = b'-';
const PLUS: u8 = b'+';
const DOT: u8 = b'.';
const EXP: u8 = b'e';
const EXP_UPPER: u8 = b'E';

fn like_number(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() { return false; }
    let code = bytes[0];
    if code == PLUS || code == MINUS {
        let next = bytes.get(1).copied().unwrap_or(0);
        if (48..=57).contains(&next) { return true; }
        let next_next = bytes.get(2).copied().unwrap_or(0);
        if next == DOT && (48..=57).contains(&next_next) { return true; }
        return false;
    }
    if code == DOT {
        let next = bytes.get(1).copied().unwrap_or(0);
        return (48..=57).contains(&next);
    }
    (48..=57).contains(&code)
}

pub fn parse_unit(value: &str) -> Option<ParsedUnit> {
    let bytes = value.as_bytes();
    let length = bytes.len();
    if length == 0 || !like_number(value) { return None; }

    let mut pos = 0usize;
    let mut code = bytes[pos];
    if code == PLUS || code == MINUS { pos += 1; }
    while pos < length {
        code = bytes[pos];
        if !(48..=57).contains(&code) { break; }
        pos += 1;
    }
    code = bytes.get(pos).copied().unwrap_or(0);
    let next_code = bytes.get(pos + 1).copied().unwrap_or(0);

    if code == DOT && (48..=57).contains(&next_code) {
        pos += 2;
        while pos < length {
            code = bytes[pos];
            if !(48..=57).contains(&code) { break; }
            pos += 1;
        }
    }

    code = bytes.get(pos).copied().unwrap_or(0);
    let next_code = bytes.get(pos + 1).copied().unwrap_or(0);
    let next_next_code = bytes.get(pos + 2).copied().unwrap_or(0);

    let exp_match = (code == EXP || code == EXP_UPPER) && (
        (48..=57).contains(&next_code)
        || ((next_code == PLUS || next_code == MINUS) && (48..=57).contains(&next_next_code))
    );
    if exp_match {
        pos += if next_code == PLUS || next_code == MINUS { 3 } else { 2 };
        while pos < length {
            code = bytes[pos];
            if !(48..=57).contains(&code) { break; }
            pos += 1;
        }
    }

    Some(ParsedUnit {
        number: value[..pos].to_string(),
        unit: value[pos..].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pixels() {
        assert_eq!(parse_unit("16px"), Some(ParsedUnit { number: "16".into(), unit: "px".into() }));
    }
    #[test]
    fn negative_decimal() {
        assert_eq!(parse_unit("-1.5rem"), Some(ParsedUnit { number: "-1.5".into(), unit: "rem".into() }));
    }
    #[test]
    fn no_number() {
        assert_eq!(parse_unit("rem"), None);
    }
    #[test]
    fn exponent() {
        assert_eq!(parse_unit("1e2px"), Some(ParsedUnit { number: "1e2".into(), unit: "px".into() }));
    }
    #[test]
    fn signed_exponent() {
        assert_eq!(parse_unit("1.5e-2%"), Some(ParsedUnit { number: "1.5e-2".into(), unit: "%".into() }));
    }
    #[test]
    fn dot_only() {
        assert!(parse_unit(".").is_none());
    }
}
