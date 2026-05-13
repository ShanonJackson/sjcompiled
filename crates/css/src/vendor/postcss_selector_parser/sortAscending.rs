//! Port of `postcss-selector-parser/dist/sortAscending.js`.
pub fn sort_ascending<T: Ord + Clone>(list: &mut Vec<T>) { list.sort(); }
