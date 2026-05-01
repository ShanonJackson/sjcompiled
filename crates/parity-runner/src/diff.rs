//! Byte-level diff reporter. Locates the smallest divergent byte range
//! and prints it with surrounding context.

#[derive(Debug, Clone)]
pub struct DiffResult {
    pub equal: bool,
    /// Byte index of the first divergence, if any.
    pub first_diff_at: Option<usize>,
    /// Human-readable summary.
    pub summary: String,
}

pub fn diff_summary(input_label: &str, js_out: &str, rs_out: &str) -> DiffResult {
    if js_out == rs_out {
        return DiffResult { equal: true, first_diff_at: None, summary: String::new() };
    }
    let js_b = js_out.as_bytes();
    let rs_b = rs_out.as_bytes();
    let mut idx = 0;
    while idx < js_b.len() && idx < rs_b.len() && js_b[idx] == rs_b[idx] {
        idx += 1;
    }
    let context = 40;
    let start = idx.saturating_sub(context);
    let end_js = (idx + context).min(js_b.len());
    let end_rs = (idx + context).min(rs_b.len());
    let js_window = String::from_utf8_lossy(&js_b[start..end_js]);
    let rs_window = String::from_utf8_lossy(&rs_b[start..end_rs]);
    let summary = format!(
        "[{}] DIVERGE at byte {idx}\n  JS:   {:?}\n  RUST: {:?}\n  (JS len={}, RS len={})",
        input_label,
        js_window,
        rs_window,
        js_b.len(),
        rs_b.len(),
    );
    DiffResult { equal: false, first_diff_at: Some(idx), summary }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_strings() {
        let r = diff_summary("test", "abc", "abc");
        assert!(r.equal);
        assert!(r.first_diff_at.is_none());
    }

    #[test]
    fn finds_first_byte() {
        let r = diff_summary("test", "abcXef", "abcYef");
        assert!(!r.equal);
        assert_eq!(r.first_diff_at, Some(3));
        assert!(r.summary.contains("DIVERGE at byte 3"));
    }

    #[test]
    fn length_mismatch() {
        let r = diff_summary("test", "abc", "abcd");
        assert!(!r.equal);
        assert_eq!(r.first_diff_at, Some(3));
    }
}
