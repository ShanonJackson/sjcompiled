//! Port of `browserslist/node.js`. Config resolution: env vars,
//! package.json `browserslist` field, `.browserslistrc` discovery.
//!
//! Per `crates/PARITY_VERSIONS.md` Anomaly #4, defaults match the exact
//! `browserslist@4.24.4` defaults.

use crate::error::BrowserslistError;
use indexmap::IndexMap;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// `browserslist@4.24.4` line 477: `['> 0.5%', 'last 2 versions', 'Firefox ESR', 'not dead']`.
pub static DEFAULT_QUERIES: &[&str] = &["> 0.5%", "last 2 versions", "Firefox ESR", "not dead"];

pub fn default_query() -> String { DEFAULT_QUERIES.join(", ") }

/// Match upstream `parseConfig(string)` (line 333).
/// Returns a section -> queries map (`defaults` is the implicit base).
pub fn parse_config(input: &str) -> Result<IndexMap<String, Vec<String>>, BrowserslistError> {
    static SECTION_RE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"^\s*\[(.+)\]\s*$").unwrap());
    static COMMENT_RE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"#[^\n]*").unwrap());

    let stripped = COMMENT_RE.replace_all(input, "").to_string();
    let mut result: IndexMap<String, Vec<String>> = IndexMap::new();
    result.insert("defaults".to_string(), Vec::new());
    let mut current_sections: Vec<String> = vec!["defaults".to_string()];

    for raw_line in stripped.split(|c| c == '\n' || c == ',') {
        let line = raw_line.trim();
        if line.is_empty() { continue; }
        if let Some(caps) = SECTION_RE.captures(line) {
            let names: Vec<String> = caps.get(1).unwrap().as_str().trim().split(' ').map(String::from).collect();
            for name in &names {
                if result.contains_key(name) {
                    return Err(BrowserslistError {
                        message: format!("Duplicate section {} in Browserslist config", name)
                    });
                }
                result.insert(name.clone(), Vec::new());
            }
            current_sections = names;
        } else {
            for sec in &current_sections {
                result.get_mut(sec).unwrap().push(line.to_string());
            }
        }
    }
    Ok(result)
}

/// `parsePackage(file)` upstream — pulls `browserslist` field from a
/// package.json text body.
pub fn parse_package(text: &str, file_label: &str) -> Result<Option<IndexMap<String, Vec<String>>>, BrowserslistError> {
    let stripped: String = if let Some(s) = text.strip_prefix('\u{FEFF}') { s.to_string() } else { text.to_string() };
    let parsed: Value = match serde_json::from_str(&stripped) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if let Some(bl) = parsed.get("browserlist") {
        if !parsed.get("browserslist").is_some() && !bl.is_null() {
            return Err(BrowserslistError {
                message: format!("`browserlist` key instead of `browserslist` in {}", file_label),
            });
        }
    }
    let raw = match parsed.get("browserslist") { Some(v) => v, None => return Ok(None) };
    let mut out: IndexMap<String, Vec<String>> = IndexMap::new();
    match raw {
        Value::Array(arr) => {
            let mut v: Vec<String> = Vec::new();
            for item in arr {
                if let Value::String(s) = item { v.push(s.clone()); }
                else { return Err(BrowserslistError { message: "Browserslist config should be a string or an array of strings with browser queries".into() }); }
            }
            out.insert("defaults".to_string(), v);
        }
        Value::String(s) => { out.insert("defaults".to_string(), vec![s.clone()]); }
        Value::Object(map) => {
            for (k, v) in map {
                match v {
                    Value::Array(arr) => {
                        let mut sv: Vec<String> = Vec::new();
                        for item in arr {
                            if let Value::String(s) = item { sv.push(s.clone()); }
                            else { return Err(BrowserslistError { message: "Browserslist config should be a string or an array of strings with browser queries".into() }); }
                        }
                        out.insert(k.clone(), sv);
                    }
                    Value::String(s) => { out.insert(k.clone(), vec![s.clone()]); }
                    _ => return Err(BrowserslistError { message: "Browserslist config should be a string or an array of strings with browser queries".into() }),
                }
            }
        }
        _ => return Err(BrowserslistError { message: "Browserslist config should be a string or an array of strings with browser queries".into() }),
    }
    Ok(Some(out))
}

/// `findConfigFile(from)` upstream — walks ancestors looking for
/// `.browserslistrc`, `browserslist`, or `package.json#browserslist`.
pub fn find_config_file(from: &Path) -> Option<PathBuf> {
    let mut cur: Option<&Path> = Some(from);
    while let Some(dir) = cur {
        let rc = dir.join(".browserslistrc");
        let bl = dir.join("browserslist");
        let pkg = dir.join("package.json");
        let pkg_has = pkg.is_file() && {
            let body = fs::read_to_string(&pkg).unwrap_or_default();
            parse_package(&body, &pkg.to_string_lossy()).map(|o| o.is_some()).unwrap_or(false)
        };
        if bl.is_file() && pkg_has { return None; } // upstream throws — we silently skip in shim path
        if rc.is_file() && pkg_has { return None; }
        if bl.is_file() && rc.is_file() { return None; }
        if bl.is_file() { return Some(bl); }
        if rc.is_file() { return Some(rc); }
        if pkg_has { return Some(pkg); }
        cur = dir.parent();
    }
    None
}

/// `pickEnv(config, opts)` upstream — picks the right section.
pub fn pick_env<'a>(config: &'a IndexMap<String, Vec<String>>, env: Option<&str>) -> Option<&'a Vec<String>> {
    let name: String = if let Some(e) = env { e.to_string() }
        else if let Ok(v) = std::env::var("BROWSERSLIST_ENV") { v }
        else if let Ok(v) = std::env::var("NODE_ENV") { v }
        else { "production".to_string() };
    config.get(&name).or_else(|| config.get("defaults"))
}

/// `loadConfig(opts)` upstream — env > BROWSERSLIST_CONFIG > path discovery.
pub fn load_config(path: Option<&Path>, env: Option<&str>) -> Option<Vec<String>> {
    if let Ok(v) = std::env::var("BROWSERSLIST") { return Some(vec![v]); }
    if let Ok(file) = std::env::var("BROWSERSLIST_CONFIG") {
        let p = PathBuf::from(file);
        return read_config_file(&p, env);
    }
    let from = path?;
    let config_file = find_config_file(from)?;
    read_config_file(&config_file, env)
}

fn read_config_file(file: &Path, env: Option<&str>) -> Option<Vec<String>> {
    let body = fs::read_to_string(file).ok()?;
    let cfg = if file.file_name().map(|n| n == "package.json").unwrap_or(false) {
        parse_package(&body, &file.to_string_lossy()).ok().flatten()?
    } else {
        parse_config(&body).ok()?
    };
    pick_env(&cfg, env).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_simple() {
        let cfg = parse_config("> 1%\nlast 2 versions").unwrap();
        assert_eq!(cfg["defaults"], vec!["> 1%", "last 2 versions"]);
    }

    #[test]
    fn parse_config_section() {
        let cfg = parse_config("> 1%\n[modern]\nlast 1 chrome version").unwrap();
        assert_eq!(cfg["defaults"], vec!["> 1%"]);
        assert_eq!(cfg["modern"], vec!["last 1 chrome version"]);
    }

    #[test]
    fn parse_config_strips_comments() {
        let cfg = parse_config("# comment\n> 1%").unwrap();
        assert_eq!(cfg["defaults"], vec!["> 1%"]);
    }

    #[test]
    fn parse_package_array() {
        let txt = r#"{ "browserslist": ["> 1%", "ie 11"] }"#;
        let cfg = parse_package(txt, "package.json").unwrap().unwrap();
        assert_eq!(cfg["defaults"], vec!["> 1%", "ie 11"]);
    }

    #[test]
    fn parse_package_object_envs() {
        let txt = r#"{ "browserslist": { "production": ["> 1%"], "development": ["last 1 chrome version"] } }"#;
        let cfg = parse_package(txt, "package.json").unwrap().unwrap();
        assert_eq!(cfg["production"], vec!["> 1%"]);
        assert_eq!(cfg["development"], vec!["last 1 chrome version"]);
    }

    #[test]
    fn parse_package_typo_detection() {
        let txt = r#"{ "browserlist": ["> 1%"] }"#;
        let err = parse_package(txt, "package.json").unwrap_err();
        assert!(err.message.contains("`browserlist` key instead of `browserslist`"));
    }

    #[test]
    fn defaults_string() {
        assert_eq!(default_query(), "> 0.5%, last 2 versions, Firefox ESR, not dead");
    }
}
