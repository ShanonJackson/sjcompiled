Phase 0 — fully startable, none block on the CSS port.
1. Pin swc_core against @swc/core@1.15.8. Cross-reference the SWC compatibility matrix, write the pin to crates/PARITY_VERSIONS.md. ~1 hour.
2. Stand up the parity harness skeleton (crates/babel-plugin/tests/parity.rs): loads a Babel fixture, runs Babel pipeline → A, runs SWC pipeline (initially pass-through) → B, runs prettier on both, asserts byte-equality,      
   prints smallest divergent byte range on fail. Use a single trivial fixture to start; the corpus comes later.
3. Babel-against-itself baseline. Run the existing test corpus through babel-plugin + prettier in a loop and assert determinism across machines. Any flap here is a blocker that must be fixed before any port work — this exposes
   things like process.env.TEST_PKG_VERSION non-determinism that would silently sabotage Phase 1+.
4. Snapshot every existing Babel test as a fixture ((input, opts) → output). Both packages: ~50+ in babel-plugin/, 38 in babel-plugin-strip-runtime/. Mechanical work; produces the corpus the harness will run against.
5. Run the nine §3.9.14 probes that don't depend on the plugin existing yet: WASI sync I/O, mtime, transformSync ABI, instance-teardown, race, scratch-dir reachability, postcard round-trip, byte-cap eviction, resolver
   difference matrix. Each is a small standalone test crate / JS test. Probe #6 (scratch-dir reachability) is the highest-priority of these — if it fails on Windows we need to know now, not after Phase 1.
6. STATE_MUTATIONS.md enumeration. Pure grep + classification work over packages/babel-plugin/src/utils/evaluate-expression.ts and traverse_expression/. The output validates whether the §3.9.8 StateDiff enum needs more        
   variants. High-priority because if it surfaces 30 mutation kinds instead of 5, the cache architecture changes.
7. Resolver difference matrix (probe #9). Build a corpus of representative resolution requests, run them through enhanced-resolve@5.x (as createDefaultResolver configures it), npm resolve.sync(), and oxc_resolver. Document    
   divergences. This is the gate before Phase 5 and one of the highest-risk unknowns in the plan.
8. scripts/audit-included-files.ts. Instrument the existing JS plugin's onIncludedFiles, walk the consuming monorepo, realpath-canonicalize every included path, count outliers that escape the invocation cwd. The plan claims   
   ~100 outliers — verifying this count now lets us decide whether the WASI cwd preopen architecture even works for the actual workload. If the real number is 10K, the architecture changes.

Phase 1 — fully startable. babel-plugin-strip-runtime (6 files, ~600 LOC) doesn't need the CSS port at all. Standing this up validates the WASI build, sidecar JSON shape, prettier oracle on real fixtures, and the Parcel       
wrapper plumbing. Best use of calendar time while waiting.

Phase 2 — fully startable after Phase 1. Crate scaffold + dispatcher + pass-through visitor for babel-plugin. Validates that pass-through is byte-equal before any handler logic exists.

Phase 3 (hash parity test corpus) — partially startable now. We can build the corpus of (input, expected_hash) test vectors against the JS hash today; consuming the Rust hash from crates/sjcompiled-utils waits on the CSS-port
agent shipping that crate. The corpus build is mechanical and frozen forever once captured.

compat/generator.rs coverage manifest (Phase 4 entry gate) — startable now. Pure grep over the consuming monorepo for every keyframes(...) call site, classify the AST shape inside each. This is the single biggest unknown in   
the plan's calendar estimate; bounding it before Phase 4 starts means Phase 4's "3-4 weeks" stops being a guess.

Phase 5 prep — partially startable. The ~100-file consuming-monorepo refactor (so all included files resolve under cwd) blocks Phase 5 ship but not Phase 5 design work. Audit script (#8 above) feeds this.

Things that cannot start without the CSS port:

- Phase 4 task 0 (integration parity test against transform_css).
- Phase 4 task 2 (utils/css_builders.rs port — uses transform_css).
- Phase 3 exit gate (consumes the shared hash from the CSS port's crate).

My recommendation for ordering the work that's available now:

1. Probe #6 (scratch-dir reachability) and the audit script (#8) on day one. Both can invalidate large parts of the architecture. Cheap to run, expensive to discover failing in week 8.
2. Parity harness skeleton + Babel-against-itself baseline (#2, #3). Nothing else is verifiable without these.
3. STATE_MUTATIONS.md and resolver difference matrix in parallel. Both are pure-investigation tasks with no code dependencies; running them concurrently surfaces architectural risks early.
4. COMPAT_GENERATOR_COVERAGE.md. Bounds the Phase 4 unknown before Phase 4 starts.
5. Phase 1 (strip-runtime) end-to-end as the first real port. Validates the entire toolchain on a small, CSS-port-independent target.
6. Phase 2 scaffold in parallel with the tail of Phase 1 if a second engineer is available.

That stack keeps everyone busy for ~6-8 weeks before the CSS port becomes the critical-path dependency, and it front-loads every architectural unknown so by the time Phase 4 starts there are zero open questions.

Want me to start any of these now? My suggestion: kick off (1) and (8) in parallel since they're the highest-risk, cheapest-to-run validations and either could force a plan revision.

