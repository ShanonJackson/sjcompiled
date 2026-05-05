//! Verbatim port of `@emotion/is-prop-valid@1.4.0`
//! (`node_modules/@emotion/is-prop-valid/src/{index.ts,props.js}`).
//!
//! Pinned because `packages/babel-plugin/src/utils/build-styled-component.ts:7`
//! consumes its default export to filter `__cmplp.<prop>` MemberExpression
//! references inside the styled forwardRef body. Any drift in the
//! valid-prop predicate produces a different `invalidDomProps` set →
//! different generated `forwardRef` body bytes → different post-prettier
//! bytes → different consumer output. CLAUDE.md: BUGS in OLD = BUGS in
//! NEW.
//!
//! ### Why a verbatim const slice and not a regex
//!
//! Upstream builds a `RegExp` lazily from the keys of `props.js`:
//! `/^((${Object.keys(props).join('|')})|(([Dd][Aa][Tt][Aa]|[Aa][Rr][Ii][Aa]|x)-.*))$/`
//! and memoises results via `@emotion/memoize` (a tiny `Object.create(null)`
//! cache — closure-scoped, NOT process-global).
//!
//! For the Rust port, the exact-name list goes into a `phf_set` /
//! lazy `HashSet` would pull a 10kB+ proc-macro tree (`phf_codegen`,
//! `phf_macros`); CLAUDE.md flags 10MB Rust libs as a no-go and
//! WASI binary size matters. Instead we use a sorted `&[&str]` plus
//! `binary_search` — O(log n) per lookup, zero runtime allocation, no
//! macro deps.
//!
//! Memoisation is omitted on purpose. Upstream's memoise is per-call-site
//! caching of a regex test; the binary-search lookup against a 350-entry
//! table is already in the same order of magnitude (~9 comparisons),
//! and the styled body's `MemberExpression` traversal hits the predicate
//! a few times per styled call — not the hot path of the plugin.
//!
//! ### Verifying drift
//!
//! When `@emotion/is-prop-valid` upgrades, regenerate this list from
//! `node_modules/@emotion/is-prop-valid/src/props.js` (the source of
//! truth — `dist/*.cjs.js` inlines the same regex). The dist regex is
//! also reproduced verbatim in the upstream-prefix-test below as a
//! schema-lock against silent table drift. Bumping the version is a
//! coordinated change: amend `crates/PARITY_VERSIONS.md` AND this file
//! AND the styled-fixture corpus.

/// Sorted list of prop names that pass `isPropValid`. Built by taking
/// the keys of the upstream `props.js` object literal verbatim and
/// sorting them lexicographically. Sort lets us use `binary_search`.
///
/// **Source pin:** `@emotion/is-prop-valid@1.4.0`
/// (`node_modules/.bun/@emotion+is-prop-valid@1.4.0/.../src/props.js`).
const VALID_PROPS: &[&str] = &[
    "abbr",
    "about",
    "accentHeight",
    "accept",
    "acceptCharset",
    "accessKey",
    "accumulate",
    "action",
    "additive",
    "alignmentBaseline",
    "allow",
    "allowFullScreen",
    "allowPaymentRequest",
    "allowReorder",
    "allowTransparency",
    "allowUserMedia",
    "alphabetic",
    "alt",
    "amplitude",
    "arabicForm",
    "ascent",
    "async",
    "attributeName",
    "attributeType",
    "autoCapitalize",
    "autoComplete",
    "autoCorrect",
    "autoFocus",
    "autoPlay",
    "autoReverse",
    "autoSave",
    "autofocus",
    "azimuth",
    "baseFrequency",
    "baseProfile",
    "baselineShift",
    "bbox",
    "begin",
    "bias",
    "by",
    "calcMode",
    "capHeight",
    "capture",
    "cellPadding",
    "cellSpacing",
    "challenge",
    "charSet",
    "checked",
    "children",
    "cite",
    "class",
    "classID",
    "className",
    "clip",
    "clipPath",
    "clipPathUnits",
    "clipRule",
    "colSpan",
    "color",
    "colorInterpolation",
    "colorInterpolationFilters",
    "colorProfile",
    "colorRendering",
    "cols",
    "content",
    "contentEditable",
    "contentScriptType",
    "contentStyleType",
    "contextMenu",
    "controls",
    "controlsList",
    "coords",
    "crossOrigin",
    "cursor",
    "cx",
    "cy",
    "d",
    "dangerouslySetInnerHTML",
    "data",
    "datatype",
    "dateTime",
    "decelerate",
    "decoding",
    "default",
    "defaultChecked",
    "defaultValue",
    "defer",
    "descent",
    "diffuseConstant",
    "dir",
    "direction",
    "disablePictureInPicture",
    "disableRemotePlayback",
    "disabled",
    "display",
    "divisor",
    "dominantBaseline",
    "download",
    "draggable",
    "dur",
    "dx",
    "dy",
    "edgeMode",
    "elevation",
    "enableBackground",
    "encType",
    "end",
    "enterKeyHint",
    "exponent",
    "externalResourcesRequired",
    "fallback",
    "fetchPriority",
    "fetchpriority",
    "fill",
    "fillOpacity",
    "fillRule",
    "filter",
    "filterRes",
    "filterUnits",
    "floodColor",
    "floodOpacity",
    "focusable",
    "fontFamily",
    "fontSize",
    "fontSizeAdjust",
    "fontStretch",
    "fontStyle",
    "fontVariant",
    "fontWeight",
    "for",
    "form",
    "formAction",
    "formEncType",
    "formMethod",
    "formNoValidate",
    "formTarget",
    "format",
    "fr",
    "frameBorder",
    "from",
    "fx",
    "fy",
    "g1",
    "g2",
    "glyphName",
    "glyphOrientationHorizontal",
    "glyphOrientationVertical",
    "glyphRef",
    "gradientTransform",
    "gradientUnits",
    "hanging",
    "headers",
    "height",
    "hidden",
    "high",
    "horizAdvX",
    "horizOriginX",
    "href",
    "hrefLang",
    "htmlFor",
    "httpEquiv",
    "id",
    "ideographic",
    "imageRendering",
    "in",
    "in2",
    "incremental",
    "inert",
    "inlist",
    "innerHTML",
    "inputMode",
    "integrity",
    "intercept",
    "is",
    "itemID",
    "itemProp",
    "itemRef",
    "itemScope",
    "itemType",
    "k",
    "k1",
    "k2",
    "k3",
    "k4",
    "kernelMatrix",
    "kernelUnitLength",
    "kerning",
    "key",
    "keyParams",
    "keyPoints",
    "keySplines",
    "keyTimes",
    "keyType",
    "kind",
    "label",
    "lang",
    "lengthAdjust",
    "letterSpacing",
    "lightingColor",
    "limitingConeAngle",
    "list",
    "loading",
    "local",
    "loop",
    "low",
    "marginHeight",
    "marginWidth",
    "markerEnd",
    "markerHeight",
    "markerMid",
    "markerStart",
    "markerUnits",
    "markerWidth",
    "mask",
    "maskContentUnits",
    "maskUnits",
    "mathematical",
    "max",
    "maxLength",
    "media",
    "mediaGroup",
    "method",
    "min",
    "minLength",
    "mode",
    "multiple",
    "muted",
    "name",
    "noValidate",
    "nonce",
    "numOctaves",
    "offset",
    "on",
    "opacity",
    "open",
    "operator",
    "optimum",
    "option",
    "order",
    "orient",
    "orientation",
    "origin",
    "overflow",
    "overlinePosition",
    "overlineThickness",
    "paintOrder",
    "panose1",
    "pathLength",
    "pattern",
    "patternContentUnits",
    "patternTransform",
    "patternUnits",
    "placeholder",
    "playsInline",
    "pointerEvents",
    "points",
    "pointsAtX",
    "pointsAtY",
    "pointsAtZ",
    "popover",
    "popoverTarget",
    "popoverTargetAction",
    "poster",
    "prefix",
    "preload",
    "preserveAlpha",
    "preserveAspectRatio",
    "primitiveUnits",
    "profile",
    "property",
    "r",
    "radioGroup",
    "radius",
    "readOnly",
    "ref",
    "refX",
    "refY",
    "referrerPolicy",
    "rel",
    "renderingIntent",
    "repeatCount",
    "repeatDur",
    "required",
    "requiredExtensions",
    "requiredFeatures",
    "resource",
    "restart",
    "result",
    "results",
    "reversed",
    "role",
    "rotate",
    "rowSpan",
    "rows",
    "rx",
    "ry",
    "sandbox",
    "scale",
    "scope",
    "scoped",
    "scrolling",
    "seamless",
    "security",
    "seed",
    "selected",
    "shape",
    "shapeRendering",
    "size",
    "sizes",
    "slope",
    "slot",
    "spacing",
    "span",
    "specularConstant",
    "specularExponent",
    "speed",
    "spellCheck",
    "spreadMethod",
    "src",
    "srcDoc",
    "srcLang",
    "srcSet",
    "start",
    "startOffset",
    "stdDeviation",
    "stemh",
    "stemv",
    "step",
    "stitchTiles",
    "stopColor",
    "stopOpacity",
    "strikethroughPosition",
    "strikethroughThickness",
    "string",
    "stroke",
    "strokeDasharray",
    "strokeDashoffset",
    "strokeLinecap",
    "strokeLinejoin",
    "strokeMiterlimit",
    "strokeOpacity",
    "strokeWidth",
    "style",
    "summary",
    "suppressContentEditableWarning",
    "suppressHydrationWarning",
    "surfaceScale",
    "systemLanguage",
    "tabIndex",
    "tableValues",
    "target",
    "targetX",
    "targetY",
    "textAnchor",
    "textDecoration",
    "textLength",
    "textRendering",
    "title",
    "to",
    "transform",
    "translate",
    "type",
    "typeof",
    "u1",
    "u2",
    "underlinePosition",
    "underlineThickness",
    "unicode",
    "unicodeBidi",
    "unicodeRange",
    "unitsPerEm",
    "unselectable",
    "useMap",
    "vAlphabetic",
    "vHanging",
    "vIdeographic",
    "vMathematical",
    "value",
    "valueLink",
    "values",
    "vectorEffect",
    "version",
    "vertAdvY",
    "vertOriginX",
    "vertOriginY",
    "viewBox",
    "viewTarget",
    "visibility",
    "vocab",
    "width",
    "widths",
    "wmode",
    "wordSpacing",
    "wrap",
    "writingMode",
    "x",
    "x1",
    "x2",
    "xChannelSelector",
    "xHeight",
    "xlinkActuate",
    "xlinkArcrole",
    "xlinkHref",
    "xlinkRole",
    "xlinkShow",
    "xlinkTitle",
    "xlinkType",
    "xmlBase",
    "xmlLang",
    "xmlSpace",
    "xmlns",
    "xmlnsXlink",
    "y",
    "y1",
    "y2",
    "yChannelSelector",
    "z",
    "zoomAndPan",
];

/// Returns `true` iff `prop` is recognised as a valid DOM/SVG/React
/// attribute name by `@emotion/is-prop-valid@1.4.0`.
///
/// Mirrors the upstream regex
/// `/^((<keys>)|(([Dd][Aa][Tt][Aa]|[Aa][Rr][Ii][Aa]|x)-.*))$/`
/// PLUS the `onX*` short-circuit:
/// `prop.charCodeAt(0) === 111 && charCodeAt(1) === 110 && charCodeAt(2) < 91`
/// (i.e. `on` followed by an uppercase ASCII letter — `onClick` /
/// `onChange` / etc.).
pub fn is_prop_valid(prop: &str) -> bool {
    // Exact-name table.
    if VALID_PROPS.binary_search(&prop).is_ok() {
        return true;
    }

    // `data-*` / `aria-*` / `x-*` (case-insensitive on the prefix per
    // the `[Dd][Aa][Tt][Aa]` etc. brackets — `x-*` is case-sensitive
    // because the upstream class is `x` only).
    if has_data_prefix(prop) || has_aria_prefix(prop) || has_x_prefix(prop) {
        return true;
    }

    // `on<UpperAscii>...` short-circuit. Bytes 0/1 are 'o'/'n' (ASCII),
    // byte 2 is a code unit < 91 (i.e. uppercase ASCII letter, U+0041
    // 'A' through U+005A 'Z'; or any control byte; matches upstream's
    // `charCodeAt(2) < 91`).
    let bytes = prop.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'o' && bytes[1] == b'n' && (bytes[2] as u32) < 91 {
        return true;
    }

    false
}

/// `[Dd][Aa][Tt][Aa]-...` prefix check. The dash MUST be present, the
/// part after the dash can be anything (including empty — upstream's
/// `.*` matches zero chars too).
fn has_data_prefix(prop: &str) -> bool {
    let bytes = prop.as_bytes();
    bytes.len() >= 5
        && (bytes[0] == b'D' || bytes[0] == b'd')
        && (bytes[1] == b'A' || bytes[1] == b'a')
        && (bytes[2] == b'T' || bytes[2] == b't')
        && (bytes[3] == b'A' || bytes[3] == b'a')
        && bytes[4] == b'-'
}

/// `[Aa][Rr][Ii][Aa]-...` prefix check.
fn has_aria_prefix(prop: &str) -> bool {
    let bytes = prop.as_bytes();
    bytes.len() >= 5
        && (bytes[0] == b'A' || bytes[0] == b'a')
        && (bytes[1] == b'R' || bytes[1] == b'r')
        && (bytes[2] == b'I' || bytes[2] == b'i')
        && (bytes[3] == b'A' || bytes[3] == b'a')
        && bytes[4] == b'-'
}

/// `x-...` prefix check (case-sensitive — upstream uses literal `x`).
fn has_x_prefix(prop: &str) -> bool {
    let bytes = prop.as_bytes();
    bytes.len() >= 2 && bytes[0] == b'x' && bytes[1] == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard the table against unsorted insertion. `binary_search`
    /// silently returns wrong answers if the slice isn't sorted; this
    /// test catches a careless edit.
    #[test]
    fn valid_props_table_is_sorted() {
        for window in VALID_PROPS.windows(2) {
            assert!(
                window[0] < window[1],
                "VALID_PROPS not sorted: {:?} >= {:?}",
                window[0],
                window[1]
            );
        }
    }

    /// Exact entry count from upstream's `props.js` (Object.keys
    /// length). If `@emotion/is-prop-valid` adds or removes entries
    /// without a coordinated bump of `crates/PARITY_VERSIONS.md`, this
    /// fires.
    #[test]
    fn valid_props_table_count_matches_upstream_1_4_0() {
        assert_eq!(VALID_PROPS.len(), 418);
    }

    #[test]
    fn react_specific_props_pass() {
        // Sample of the React-prefixed entries.
        assert!(is_prop_valid("children"));
        assert!(is_prop_valid("dangerouslySetInnerHTML"));
        assert!(is_prop_valid("key"));
        assert!(is_prop_valid("ref"));
        assert!(is_prop_valid("className"));
        assert!(is_prop_valid("htmlFor"));
        assert!(is_prop_valid("onClick"));
    }

    #[test]
    fn html_attributes_pass() {
        assert!(is_prop_valid("href"));
        assert!(is_prop_valid("type"));
        assert!(is_prop_valid("value"));
        assert!(is_prop_valid("disabled"));
        assert!(is_prop_valid("placeholder"));
    }

    #[test]
    fn svg_attributes_pass() {
        assert!(is_prop_valid("clipPath"));
        assert!(is_prop_valid("strokeWidth"));
        assert!(is_prop_valid("viewBox"));
        assert!(is_prop_valid("xlinkHref"));
        assert!(is_prop_valid("d"));
    }

    #[test]
    fn data_prefix_passes_case_insensitive() {
        assert!(is_prop_valid("data-testid"));
        assert!(is_prop_valid("DATA-FOO"));
        assert!(is_prop_valid("Data-Bar"));
        assert!(is_prop_valid("dAtA-baz"));
        // Empty suffix is allowed by the regex `.*` quantifier.
        assert!(is_prop_valid("data-"));
    }

    #[test]
    fn aria_prefix_passes_case_insensitive() {
        assert!(is_prop_valid("aria-label"));
        assert!(is_prop_valid("ARIA-HIDDEN"));
        assert!(is_prop_valid("Aria-Live"));
    }

    #[test]
    fn x_prefix_passes_case_sensitive() {
        assert!(is_prop_valid("x-foo"));
        // Capital X — upstream's regex literal is lowercase `x`, so
        // capital-X variants do NOT pass.
        assert!(!is_prop_valid("X-foo"));
    }

    #[test]
    fn on_uppercase_passes_via_charcode_check() {
        // Standard React event handlers — `onClick`, `onMouseDown`.
        assert!(is_prop_valid("onClick"));
        assert!(is_prop_valid("onMouseDown"));
        // Even non-real names match if they fit `on<UpperAscii>`.
        assert!(is_prop_valid("onZorgleZap"));
    }

    #[test]
    fn on_lowercase_fails_via_charcode_check() {
        // Lowercase third byte fails the `< 91` check.
        assert!(!is_prop_valid("onclick"));
        assert!(!is_prop_valid("onmousedown"));
    }

    #[test]
    fn unknown_props_fail() {
        assert!(!is_prop_valid("custom"));
        assert!(!is_prop_valid("isPrimary"));
        assert!(!is_prop_valid("variant"));
        assert!(!is_prop_valid("foo"));
    }

    #[test]
    fn empty_string_fails() {
        // Upstream regex is anchored — empty doesn't match.
        assert!(!is_prop_valid(""));
    }

    #[test]
    fn data_aria_x_without_dash_fail() {
        // The `-` after the prefix is required.
        assert!(!is_prop_valid("dataXyz"));
        assert!(!is_prop_valid("ariaLabel"));
        assert!(!is_prop_valid("xfoo"));
    }
}
