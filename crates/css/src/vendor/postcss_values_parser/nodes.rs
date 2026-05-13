//! Port of `postcss-values-parser/lib/nodes/*.js`.
//!
//! Folder/file mapping (1:1 with upstream `lib/nodes/`):
//!   - `AtWord.js`         -> `at_word.rs`
//!   - `Comment.js`        -> `comment.rs`
//!   - `Container.js`      -> `container.rs`
//!   - `Func.js`           -> `func.rs`
//!   - `Interpolation.js`  -> `interpolation.rs`
//!   - `Node.js`           -> `node.rs`
//!   - `Numeric.js`        -> `numeric.rs`
//!   - `Operator.js`       -> `operator.rs`
//!   - `Punctuation.js`    -> `punctuation.rs`
//!   - `Quoted.js`         -> `quoted.rs`
//!   - `UnicodeRange.js`   -> `unicode_range.rs`
//!   - `Word.js`           -> `word.rs`

pub mod at_word;
pub mod comment;
pub mod container;
pub mod func;
pub mod interpolation;
pub mod node;
pub mod numeric;
pub mod operator;
pub mod punctuation;
pub mod quoted;
pub mod unicode_range;
pub mod word;

#[derive(Debug, Clone)]
pub enum NodeKind {
    Root,
    AtWord(at_word::AtWord),
    Comment(comment::Comment),
    Func(func::Func),
    Interpolation(interpolation::Interpolation),
    Numeric(numeric::Numeric),
    Operator(operator::Operator),
    Punctuation(punctuation::Punctuation),
    Quoted(quoted::Quoted),
    UnicodeRange(unicode_range::UnicodeRange),
    Word(word::Word),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub raws_before: String,
    pub raws_after: String,
    pub source_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Root {
    pub nodes: Vec<Node>,
    pub raw_value: Option<String>,
}

impl Root {
    pub fn new() -> Self { Root::default() }
}
