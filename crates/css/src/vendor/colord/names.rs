//! Port of `colord/plugins/names.js`.
//!
//! Maps the 148 CSS named colors to their hex strings, plus handles the
//! special `transparent` keyword. The order below mirrors upstream's
//! `var a = { ... }` literal byte-for-byte so iteration order matches when
//! callers walk it (e.g. `closest()` in the names plugin).

use super::types::RgbaColor;
use indexmap::IndexMap;
use once_cell::sync::Lazy;

pub static NAME_TO_HEX: Lazy<IndexMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = IndexMap::new();
    m.insert("white", "#ffffff");
    m.insert("bisque", "#ffe4c4");
    m.insert("blue", "#0000ff");
    m.insert("cadetblue", "#5f9ea0");
    m.insert("chartreuse", "#7fff00");
    m.insert("chocolate", "#d2691e");
    m.insert("coral", "#ff7f50");
    m.insert("antiquewhite", "#faebd7");
    m.insert("aqua", "#00ffff");
    m.insert("azure", "#f0ffff");
    m.insert("whitesmoke", "#f5f5f5");
    m.insert("papayawhip", "#ffefd5");
    m.insert("plum", "#dda0dd");
    m.insert("blanchedalmond", "#ffebcd");
    m.insert("black", "#000000");
    m.insert("gold", "#ffd700");
    m.insert("goldenrod", "#daa520");
    m.insert("gainsboro", "#dcdcdc");
    m.insert("cornsilk", "#fff8dc");
    m.insert("cornflowerblue", "#6495ed");
    m.insert("burlywood", "#deb887");
    m.insert("aquamarine", "#7fffd4");
    m.insert("beige", "#f5f5dc");
    m.insert("crimson", "#dc143c");
    m.insert("cyan", "#00ffff");
    m.insert("darkblue", "#00008b");
    m.insert("darkcyan", "#008b8b");
    m.insert("darkgoldenrod", "#b8860b");
    m.insert("darkkhaki", "#bdb76b");
    m.insert("darkgray", "#a9a9a9");
    m.insert("darkgreen", "#006400");
    m.insert("darkgrey", "#a9a9a9");
    m.insert("peachpuff", "#ffdab9");
    m.insert("darkmagenta", "#8b008b");
    m.insert("darkred", "#8b0000");
    m.insert("darkorchid", "#9932cc");
    m.insert("darkorange", "#ff8c00");
    m.insert("darkslateblue", "#483d8b");
    m.insert("gray", "#808080");
    m.insert("darkslategray", "#2f4f4f");
    m.insert("darkslategrey", "#2f4f4f");
    m.insert("deeppink", "#ff1493");
    m.insert("deepskyblue", "#00bfff");
    m.insert("wheat", "#f5deb3");
    m.insert("firebrick", "#b22222");
    m.insert("floralwhite", "#fffaf0");
    m.insert("ghostwhite", "#f8f8ff");
    m.insert("darkviolet", "#9400d3");
    m.insert("magenta", "#ff00ff");
    m.insert("green", "#008000");
    m.insert("dodgerblue", "#1e90ff");
    m.insert("grey", "#808080");
    m.insert("honeydew", "#f0fff0");
    m.insert("hotpink", "#ff69b4");
    m.insert("blueviolet", "#8a2be2");
    m.insert("forestgreen", "#228b22");
    m.insert("lawngreen", "#7cfc00");
    m.insert("indianred", "#cd5c5c");
    m.insert("indigo", "#4b0082");
    m.insert("fuchsia", "#ff00ff");
    m.insert("brown", "#a52a2a");
    m.insert("maroon", "#800000");
    m.insert("mediumblue", "#0000cd");
    m.insert("lightcoral", "#f08080");
    m.insert("darkturquoise", "#00ced1");
    m.insert("lightcyan", "#e0ffff");
    m.insert("ivory", "#fffff0");
    m.insert("lightyellow", "#ffffe0");
    m.insert("lightsalmon", "#ffa07a");
    m.insert("lightseagreen", "#20b2aa");
    m.insert("linen", "#faf0e6");
    m.insert("mediumaquamarine", "#66cdaa");
    m.insert("lemonchiffon", "#fffacd");
    m.insert("lime", "#00ff00");
    m.insert("khaki", "#f0e68c");
    m.insert("mediumseagreen", "#3cb371");
    m.insert("limegreen", "#32cd32");
    m.insert("mediumspringgreen", "#00fa9a");
    m.insert("lightskyblue", "#87cefa");
    m.insert("lightblue", "#add8e6");
    m.insert("midnightblue", "#191970");
    m.insert("lightpink", "#ffb6c1");
    m.insert("mistyrose", "#ffe4e1");
    m.insert("moccasin", "#ffe4b5");
    m.insert("mintcream", "#f5fffa");
    m.insert("lightslategray", "#778899");
    m.insert("lightslategrey", "#778899");
    m.insert("navajowhite", "#ffdead");
    m.insert("navy", "#000080");
    m.insert("mediumvioletred", "#c71585");
    m.insert("powderblue", "#b0e0e6");
    m.insert("palegoldenrod", "#eee8aa");
    m.insert("oldlace", "#fdf5e6");
    m.insert("paleturquoise", "#afeeee");
    m.insert("mediumturquoise", "#48d1cc");
    m.insert("mediumorchid", "#ba55d3");
    m.insert("rebeccapurple", "#663399");
    m.insert("lightsteelblue", "#b0c4de");
    m.insert("mediumslateblue", "#7b68ee");
    m.insert("thistle", "#d8bfd8");
    m.insert("tan", "#d2b48c");
    m.insert("orchid", "#da70d6");
    m.insert("mediumpurple", "#9370db");
    m.insert("purple", "#800080");
    m.insert("pink", "#ffc0cb");
    m.insert("skyblue", "#87ceeb");
    m.insert("springgreen", "#00ff7f");
    m.insert("palegreen", "#98fb98");
    m.insert("red", "#ff0000");
    m.insert("yellow", "#ffff00");
    m.insert("slateblue", "#6a5acd");
    m.insert("lavenderblush", "#fff0f5");
    m.insert("peru", "#cd853f");
    m.insert("palevioletred", "#db7093");
    m.insert("violet", "#ee82ee");
    m.insert("teal", "#008080");
    m.insert("slategray", "#708090");
    m.insert("slategrey", "#708090");
    m.insert("aliceblue", "#f0f8ff");
    m.insert("darkseagreen", "#8fbc8f");
    m.insert("darkolivegreen", "#556b2f");
    m.insert("greenyellow", "#adff2f");
    m.insert("seagreen", "#2e8b57");
    m.insert("seashell", "#fff5ee");
    m.insert("tomato", "#ff6347");
    m.insert("silver", "#c0c0c0");
    m.insert("sienna", "#a0522d");
    m.insert("lavender", "#e6e6fa");
    m.insert("lightgreen", "#90ee90");
    m.insert("orange", "#ffa500");
    m.insert("orangered", "#ff4500");
    m.insert("steelblue", "#4682b4");
    m.insert("royalblue", "#4169e1");
    m.insert("turquoise", "#40e0d0");
    m.insert("yellowgreen", "#9acd32");
    m.insert("salmon", "#fa8072");
    m.insert("saddlebrown", "#8b4513");
    m.insert("sandybrown", "#f4a460");
    m.insert("rosybrown", "#bc8f8f");
    m.insert("darksalmon", "#e9967a");
    m.insert("lightgoldenrodyellow", "#fafad2");
    m.insert("snow", "#fffafa");
    m.insert("lightgrey", "#d3d3d3");
    m.insert("lightgray", "#d3d3d3");
    m.insert("dimgray", "#696969");
    m.insert("dimgrey", "#696969");
    m.insert("olivedrab", "#6b8e23");
    m.insert("olive", "#808000");
    m
});

/// Reverse map for `toName()`: hex -> name. Walks `NAME_TO_HEX` in insertion
/// order so collisions (`gray`/`grey`) resolve to the *last* inserted name —
/// matches upstream's `for (var d in a) r[a[d]] = d` JS semantics.
pub static HEX_TO_NAME: Lazy<IndexMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = IndexMap::new();
    for (name, hex) in NAME_TO_HEX.iter() {
        m.insert(*hex, *name);
    }
    m
});

/// `f.string.push([function(f){var r=f.toLowerCase(),d="transparent"===r?"#0000":a[r];...},"name"])`.
/// Returns the rgba for a CSS named color (`transparent` -> `#0000`).
pub fn lookup_name(input: &str) -> Option<RgbaColor> {
    let lower = input.to_lowercase();
    let hex_target = if lower == "transparent" { "#0000" }
        else { NAME_TO_HEX.get(lower.as_str()).copied()? };
    // Re-enter the hex parser via a fresh parse call.
    super::parse::parse(&super::parse::ColordInput::Str(hex_target.to_string())).map(|(rgba, _)| rgba)
}

/// `toName()` reverse lookup — closest=false path.
pub fn rgba_to_name(rgba: super::types::RgbaColor) -> Option<&'static str> {
    if rgba.a == 0.0 && rgba.r == 0.0 && rgba.g == 0.0 && rgba.b == 0.0 {
        return Some("transparent");
    }
    // Build the lookup hex via the round_rgba + hex_pair_str helpers.
    let rounded = super::helpers::round_rgba(rgba);
    let hex = format!(
        "#{}{}{}",
        super::helpers::hex_pair_str(rounded.r as u32),
        super::helpers::hex_pair_str(rounded.g as u32),
        super::helpers::hex_pair_str(rounded.b as u32),
    );
    HEX_TO_NAME.get(hex.as_str()).copied()
}

/// `closest()` — Pythagorean nearest match in RGB space. Walks `NAME_TO_HEX`
/// in insertion order so ties resolve to the first match (upstream JS object
/// iteration order is insertion-ordered for string keys).
pub fn closest_name(rgba: super::types::RgbaColor) -> &'static str {
    let mut best = "black";
    let mut best_dist = f64::INFINITY;
    for (name, hex) in NAME_TO_HEX.iter() {
        let candidate = match super::parse::parse(&super::parse::ColordInput::Str(hex.to_string())) {
            Some((c, _)) => c,
            None => continue,
        };
        let dist = (rgba.r - candidate.r).powi(2)
            + (rgba.g - candidate.g).powi(2)
            + (rgba.b - candidate.b).powi(2);
        if dist < best_dist {
            best_dist = dist;
            best = *name;
        }
    }
    best
}
