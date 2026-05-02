//! Port of `postcss-selector-parser/dist/processor.js`.

use crate::nodes::Node;
use crate::parser::Parser;
use crate::selectors::stringify;
use crate::tokenize::TokenizeError;

#[derive(Debug, Clone, Default)]
pub struct ProcessorOptions {
    pub lossless: bool,
    pub update_selector: bool,
}

pub struct Processor;

impl Processor {
    pub fn new() -> Self { Processor }

    /// `processor.processSync(rule, opts)` style — for our purposes the
    /// caller passes a selector string and a mutator; we return the
    /// (potentially mutated) selector string.
    pub fn process<F: FnOnce(&mut Node)>(&self, selector: &str, f: F) -> Result<String, TokenizeError> {
        let mut p = Parser::new(selector.to_string(), ProcessorOptions::default());
        let root = p.parse()?;
        f(root);
        Ok(stringify(root))
    }

    pub fn ast_sync(&self, selector: &str) -> Result<Node, TokenizeError> {
        let mut p = Parser::new(selector.to_string(), ProcessorOptions::default());
        Ok(p.parse()?.clone())
    }

    /// `processor.processSync(selector)` upstream — no-closure form.
    /// Parses the selector and returns its stringified form. Used by
    /// `cssnano-postcss-minify-selectors@5.2.1` (`processor.processSync(
    /// selector)` in its `OnceExit` hook) where the mutation is performed
    /// inside the parser-callback that the consumer registers via the
    /// builder; for our flat API, callers compose `ast_sync` + manual
    /// mutation + `stringify` instead.
    pub fn process_sync(&self, selector: &str) -> Result<String, TokenizeError> {
        let mut p = Parser::new(selector.to_string(), ProcessorOptions::default());
        let root = p.parse()?;
        Ok(stringify(root))
    }
}

impl Default for Processor { fn default() -> Self { Self::new() } }
