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


## Drifts to look into AGAIN
● Drift detected in packages/css/scripts/parity-bridge.mjs — outside my work. A parallel agent added import postcssMinifySelectors from 'postcss-minify-selectors'; (line 44) but the package isn't resolvable in node_modules,     
breaking the JS bridge for ALL stages mid-session. This is unrelated to my fixes — cargo test -p cssnano-postcss-normalize-positions (20 passed) and parity-runner postcss-normalize-positions (29/29 byte-clean) both ran green  
earlier in this session before the bridge broke.

Per constraints I can't bun install to fix it. The user needs to know.                                                                                                                                                                                                                                                                                                                                                                                                
Coverage I added (3 corpus entries) for residual edge cases the existing tests didn't lock down:

- 30_empty_slot_stringify_invariant.css — value-parser stringify must NOT collapse Space nodes around an empty-value Word (the secondNode === 'center' and firstNode === 'center' apply branches both clear interior slots).        - 31_short_circuit_interactions.css — interactions among the three short-circuits (comma / slash / var()-env()-constant()) including: var-then-slash, slash-then-comma-then-keywords, trailing var, double-comma sparse-hole, full   background shorthand with gradient + slash + var.
- 32_cache_key_excludes_important.css — interleaving !important and non-!important decls with the same value, locking that the per-call cache (keyed on decl.value, which postcss strips !important from) doesn't drop or
  duplicate the !important suffix.

Other audit notes (no drift, no fix needed):

- String.prototype.toLowerCase() vs Rust str::to_lowercase() — both Unicode-default; position keywords are pure ASCII; non-ASCII firstNode/secondNode (e.g. Σ) lowercase identically in both. JS upstream does NOT use
  toLocaleLowerCase, so locale-folding (Turkish dotless-i) is not in scope.
- parseFloat leading-whitespace and 0x... edge cases — value-parser never emits Word with leading whitespace (locked by the timing-functions invariant test, which also applies here transitively); 0x1f returns true in both (JS
  parseFloat=0; Rust parse_unit("0x1f")=Some({number:"0",unit:"x1f"})).
- A parallel agent independently added their own 30_..37_ corpus entries covering: comment-between-keywords (29), JS Infinity literal (30 — duplicates my 29), comma-leading/trailing/empty-middle (31), non-math function as     
  first slot (32), non-transforming pairs (33), 4-value form (34), uppercase var/env/constant (35), uppercase calc/min/max/clamp (36), per-Root cache scope across rules (37). Their coverage is complementary; nothing to remove.  


### THIS DRIFT SHANON
Drift flagged but not mine to fix: parallel ordered-values agent's untracked crates/postcss-calc/src/lib/transform.rs references attribute_payload which doesn't exist on postcss-selector-parser::Node. They'll need to address  
it.


### THIS IS THE FULL THING
- ❌ Open gate (Task #2): oxc_browserslist's bundled caniuse-lite is ~2 chrome releases newer than the workspace pin — multi-day fix. Blocks byte-testing Prefixes::new against JS oracle.
- ⬜ Parity-runner Stage::Autoprefixer not wired
- ⬜ NAPI bridge into crates/css/src/transform.rs not wired


One stale doc — broader than this port, flagging not fixing: the legacy ## What's left to port (line 2747) and ## Recommended order for the next session (line 2767) sections still list minify-selectors, ordered-values,        
reduce-initial, minify-params, postcss-calc, postcss-colormin as pending — all shipped across the last several sessions. The phase-progress table (line 2184) is the authoritative current state. A separate housekeeping pass to
delete or rewrite those two stale sections would be cleaner than letting them drift further; I haven't touched them since it's cross-phase doc maintenance, not specific to 6f.
