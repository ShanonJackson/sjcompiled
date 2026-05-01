//! Port of `postcss/lib/declaration.js`.

#[derive(Debug, Clone, Default)]
pub struct Declaration {
    pub prop: String,
    pub value: String,
    pub important: bool,
    pub variable: bool,
}
