//! Port of `packages/utils/src/array.ts`.

/// Mirrors `unique(arr)` upstream — preserves insertion order, dedupes by
/// the supplied identity. The default identity is the value itself.
pub fn unique<T: Clone + Eq>(arr: &[T]) -> Vec<T> {
    let mut acc: Vec<T> = Vec::new();
    for item in arr {
        if !acc.iter().any(|existing| existing == item) {
            acc.push(item.clone());
        }
    }
    acc
}

/// Like `unique` but with a custom identity function (matches upstream's
/// `unique(arr, getId)` overload). Equality is over the projected key.
pub fn unique_by<T: Clone, K: Eq, F: Fn(&T) -> K>(arr: &[T], get_id: F) -> Vec<T> {
    let mut acc: Vec<T> = Vec::new();
    for item in arr {
        let id = get_id(item);
        if !acc.iter().any(|existing| get_id(existing) == id) {
            acc.push(item.clone());
        }
    }
    acc
}

/// Mirrors `flatten(...arrays)` upstream — concatenates a list of slices
/// (one level of flattening). Upstream uses `Array.prototype.reduce` which
/// preserves order.
pub fn flatten<T: Clone>(arrays: &[&[T]]) -> Vec<T> {
    let mut out = Vec::new();
    for arr in arrays {
        out.extend_from_slice(arr);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_dedupes_preserving_order() {
        let v = vec![1, 2, 2, 3, 1, 4];
        assert_eq!(unique(&v), vec![1, 2, 3, 4]);
    }

    #[test]
    fn unique_strings() {
        let v = vec!["a", "b", "a", "c"];
        assert_eq!(unique(&v), vec!["a", "b", "c"]);
    }

    #[test]
    fn unique_by_id() {
        #[derive(Clone)]
        struct Item { id: u32, label: &'static str }
        let v = vec![
            Item { id: 1, label: "x" },
            Item { id: 2, label: "y" },
            Item { id: 1, label: "z" },
        ];
        let out = unique_by(&v, |i| i.id);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].label, "x");
        assert_eq!(out[1].label, "y");
    }

    #[test]
    fn flatten_concats() {
        let a = vec![1, 2];
        let b = vec![3];
        let c = vec![4, 5, 6];
        let out = flatten(&[&a[..], &b[..], &c[..]]);
        assert_eq!(out, vec![1, 2, 3, 4, 5, 6]);
    }
}
