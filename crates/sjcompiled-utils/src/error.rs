//! Port of `packages/utils/src/error.ts`.
//!
//! `createError(packageName, group?)` returns a curried function — call it
//! with a message to produce an `Error` whose body includes the upstream
//! ASCII-art banner. We mirror by returning a closure.

/// Mirrors `createError(packageName, group)(message) -> Error`.
pub fn create_error<'a>(package_name: &'a str, group: &'a str) -> impl Fn(&str) -> String + 'a {
    let pn = package_name.to_string();
    let gp = group.to_string();
    move |message: &str| {
        let group_suffix = if gp.is_empty() { String::new() } else { format!("- {}", gp) };
        format!("\n\
 ██████╗ ██████╗ ███╗   ███╗██████╗ ██╗██╗     ███████╗██████╗\n\
██╔════╝██╔═══██╗████╗ ████║██╔══██╗██║██║     ██╔════╝██╔══██╗\n\
██║     ██║   ██║██╔████╔██║██████╔╝██║██║     █████╗  ██║  ██║\n\
██║     ██║   ██║██║╚██╔╝██║██╔═══╝ ██║██║     ██╔══╝  ██║  ██║\n\
╚██████╗╚██████╔╝██║ ╚═╝ ██║██║     ██║███████╗███████╗██████╔╝\n\
 ╚═════╝ ╚═════╝ ╚═╝     ╚═╝╚═╝     ╚═╝╚══════╝╚══════╝╚═════╝\n\
\n\
  @compiled/{} {}\n\
\n\
  {}\n", pn, group_suffix, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_package_and_message() {
        let make = create_error("css", "");
        let body = make("boom");
        assert!(body.contains("@compiled/css"));
        assert!(body.contains("boom"));
    }

    #[test]
    fn includes_group_suffix() {
        let make = create_error("css", "Unhandled exception");
        let body = make("boom");
        assert!(body.contains("- Unhandled exception"));
    }
}
