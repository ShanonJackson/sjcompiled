//! Port of `postcss-values-parser/lib/nodes/Node.js` (the abstract base).

#[derive(Debug, Clone, Default)]
pub struct Common {
    pub value: String,
    pub raws_before: String,
    pub raws_after: String,
    pub source_index: usize,
    pub source_end_index: usize,
}
