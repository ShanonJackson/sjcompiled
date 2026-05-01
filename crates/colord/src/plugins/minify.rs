//! Port of `colord/plugins/minify.js` — shortest-string serialization.
//!
//! Used by `postcss-colormin`: tries every representation (`name`, `hex`,
//! `rgb`, `hsl`) and returns whichever has the shortest byte length.

use crate::Colord;

#[derive(Debug, Clone, Default)]
pub struct MinifyOpts {
    pub hex: bool,
    pub alpha_hex: bool,
    pub rgb: bool,
    pub hsl: bool,
    pub name: bool,
    pub transparent: bool,
}

impl MinifyOpts {
    pub fn all() -> Self {
        MinifyOpts { hex: true, alpha_hex: true, rgb: true, hsl: true, name: true, transparent: true }
    }
}

pub fn minify(c: &Colord, opts: &MinifyOpts) -> String {
    let mut candidates: Vec<String> = Vec::new();
    if opts.hex { candidates.push(c.to_hex()); }
    if opts.rgb { candidates.push(c.to_rgb_string()); }
    if opts.hsl { candidates.push(c.to_hsl_string()); }
    if opts.name {
        if let Some(name) = c.to_name() { candidates.push(name.to_string()); }
    }
    if opts.transparent && c.alpha_value() == 0.0 {
        candidates.push("transparent".to_string());
    }
    candidates.into_iter().min_by_key(|s| s.len()).unwrap_or_default()
}
