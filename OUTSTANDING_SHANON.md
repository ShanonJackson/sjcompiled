## Shanon pin these to JIRA versions based on what the parcel-transformer uses through babel resolution

@babel/core
@babel/generator
@babel/parser
@babel/template
@babel/traverse
@babel/types
@babel/helper-plugin-utils
@babel/plugin-syntax-jsx
@babel/plugin-transform-flow-strip-types
@emotion/is-prop-valid

# autoprefixer
- MORNING.md file was written in crates/autoprefixer/MORNING.md that is the remainder of the work for autoprefixer.

# cssnano-postcss-normalize agent got up to here:

 ✅ #1 postcss-normalize-url@5.1.0 — DONE, byte-clean across 60-fixture corpus, deterministic JS oracle 
- ⏸ #2 postcss-minify-selectors@5.2.1 — selector-parser surface verified ready, plugin port not started
- - ⏸ #3 postcss-ordered-values@5.1.3 — pending       --- This is updated in STATUS is STATUS.md up to date before we sleep?    

One tiny inconsistency for next session (not worth touching tonight): the selector-parser extension note at the top doesn't yet say "minify-selectors is now UNBLOCKED" explicitly — but the verification gates listed (31/31     
pass, 112/112 postcss-nested, 6/6 compiled-css) make that clear by implication.



### Drifts to look into
Two pieces of DRIFT detected (both flagged, none silently worked around)

1. oxc_browserslist's caniuse-lite snapshot is ~2 chrome releases newer than the workspace pin (1.0.30001766). Concrete numbers in HANDOVER §6 + STATUS "Phase 7 ship — browserslist-shim parity gate". Closure has 3 multi-day   
   options; tracked as Task #2. This is the hard pre-condition for Prefixes::new.

----
- atomicify-rules selectors join. JS ${selectors} on an Array uses Array.prototype.toString() = Array.join(",") — comma, no space. The agent claimed Rust uses empty separator. If true, that's a hash-input divergence on
  multi-selector rules — every consumer would see different class names. High priority to verify.
- discard-empty-rules whitespace set. JS .trim() strips a specific Unicode set. Agent says Rust is_js_whitespace covers U+FEFF but didn't enumerate U+1680/U+2000-200A/U+202F/U+205F/U+3000/U+0085. Worth checking the function   
  body.
- parent-orphaned-pseudos comma split inside parens. Agent claimed both respect :is(a, b) nesting but didn't actually trace rule.get_selectors() into postcss-core. If Rust naively splits on , it will mangle :is(a, b), c into  
  :is(a, b), c.
- merge-duplicate-at-rules dedup key with raws. Agent asserted stringify_node includes raws.before without showing the code path. If it doesn't, identical decls with different leading whitespace get incorrectly deduped.