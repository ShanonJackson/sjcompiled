//! Plugin error convention.
//!
//! Plugins emit errors via [`PluginError`]. Plugin authors:
//!   * Return `Result<(), PluginError>` from plugin entrypoints.
//!   * Use [`PluginError::from_node`] when an offending node has a
//!     `source.start` so the error carries line/col info.
//!   * Use [`PluginError::generic`] for plugin-state errors not tied to a
//!     specific node (config validation, etc.).
//!
//! The error preserves upstream's `input.error('Missed semicolon', ...)`
//! shape — callers who go through NAPI marshal it to a JS Error with the
//! same fields (`message`, `line`, `column`, `plugin`, `file`).

use crate::css_syntax_error::CssSyntaxError;
use crate::node::Node;

#[derive(Debug, Clone)]
pub struct PluginError {
    pub plugin: String,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl PluginError {
    pub fn generic(plugin: impl Into<String>, message: impl Into<String>) -> Self {
        PluginError {
            plugin: plugin.into(),
            message: message.into(),
            line: None,
            column: None,
        }
    }

    /// Build an error tied to a specific node — uses `node.source.start`
    /// for line/col info if available.
    pub fn from_node(plugin: impl Into<String>, message: impl Into<String>, node: &Node) -> Self {
        let (line, column) = match &node.source.start {
            Some(p) => (Some(p.line), Some(p.column)),
            None => (None, None),
        };
        PluginError {
            plugin: plugin.into(),
            message: message.into(),
            line,
            column,
        }
    }

    /// Convert to a [`CssSyntaxError`] for callers that need to bubble
    /// this through the postcss-style error path.
    pub fn into_css_syntax_error(self) -> CssSyntaxError {
        let mut err = CssSyntaxError::new(self.message, self.line, self.column);
        err.plugin = Some(self.plugin);
        err
    }
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.plugin, self.message)?;
        if let (Some(l), Some(c)) = (self.line, self.column) {
            write!(f, " (line {l}, col {c})")?;
        }
        Ok(())
    }
}

impl std::error::Error for PluginError {}

/// Plugin entrypoints follow this convention:
///
/// ```ignore
/// pub fn my_plugin(root: &mut postcss_core::Root, opts: &MyOpts)
///     -> postcss_core::PluginResult
/// {
///     // walk, mutate, return Ok(()) on success.
///     Ok(())
/// }
/// ```
pub type PluginResult = Result<(), PluginError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_error_has_no_position() {
        let e = PluginError::generic("atomicify-rules", "config missing");
        assert_eq!(e.line, None);
        assert_eq!(e.column, None);
        assert_eq!(e.plugin, "atomicify-rules");
    }

    #[test]
    fn display_includes_plugin_and_message() {
        let e = PluginError::generic("p", "boom");
        let s = format!("{}", e);
        assert!(s.contains("[p]"));
        assert!(s.contains("boom"));
    }

    #[test]
    fn into_css_syntax_error_propagates_plugin() {
        let e = PluginError::generic("p", "x");
        let css_err = e.into_css_syntax_error();
        assert_eq!(css_err.plugin.as_deref(), Some("p"));
    }
}
