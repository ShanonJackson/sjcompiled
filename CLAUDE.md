## Goal
We've implemented packages/css IDENTICALLY in Rust exposed via packages/css-native/index.js; Performance is a lot slower, but we're investigating that in parallel.
Output through transformCss is CONFIRMED byte-equal via fixtures/* running babel with/without Rust and confirming byte-equality (See: packages/equality-harness/scripts/verify.mjs)
Now that we've confirmed packages/css has been rebuilt and packages/babel-plugin works IDENTICALLY using the new one. It's time to start working towards migrating
packages/babel-plugin and packages-babel-plugin-strip-runtime to Rust following our DEEP investigations here:  plugins/* (ALL MD FILES READ THESE).

The ONLY way to do this correctly is to migrate everything 1:1 same folder/file system as ORIGINAL where all the parts are 1:1 and therefore the WHOLE is 1:1. 
Obviously we have to make some EXTREMELY minor exceptions because babel and swc are not the same. For anything that needs to match a babel-api put that in a compat/* folder with clear comment on usage.

BUGS in OLD! Need to be BUGS In NEW. We are not fixing bugs as part of this; This is EXTREMELY intentional as if we ship something that has same output we can ship it very easily.


# DRIFT DETECTION - 
THIS PART IS CRITICAL!
If you think someone hasn't ported something OUTSIDE your work CORRECTLY; Immedietly I.E "Drift detected in X - <Explanation here>" this is CRITICALLY important. otherwise if many things have slight drift the "WHOLE" will have MAJOR drift. Minor drift is unnacceptable.
DONT try and "WORK AROUND" drift; That's not your call to make. Drift is the enemy.
Finally on 'DRIFT DETECTION' it's important we DONT try WORK AROUND and Patch drift in our implementation. This will cause more drift, not less drift. Because if 1 plugin has a small issue, and we fix the issue in ours now 2 plugins have drift instead of 1.


# Quality
Quality is more important than speed; Again if we port something even slightly incorrectly when we eventually integrate it into the 60-90GB Monorepo ANY issue that CAN happen WILL happen.

# Performance
Performance is a side-goal that should NEVER come at the cost of correctness but yet is important. If we ship the entire thing end-to-end and it's slower than the original we've failed.

# Never
- Never edit packages/babel-plugin, packages/babel-plugin-strip-runtime, packages/css and packages/utils consider them 100% IMMUTABLE as their EXACT source was copied from a monorepo.
The reason these are immutable is that EACH package was taken from the EXACT commit/version that AFM uses (the monorepo)
@compiled/babel-plugin 0.36.1 16a62b8 (solo patch release)
@compiled/babel-plugin-strip-runtime 0.36.0 40a4548 (Jan 28 batch)
@compiled/css 0.19.0 40a4548 (used by BOTH above, nested)
@compiled/utils 0.13.2 130ed3b (hoisted, shared)

One-time exception (2026-05-04): a historic fork-prefix rename was reverted to `@compiled/` across packages/* and crates/* atomically so emitted strings (runtime imports, file-header banners, error messages) match upstream. This was authorised explicitly to align with the AFM monorepo prefix. The IMMUTABLE rule still applies — no further edits to packages/* without explicit authorisation.

# WASI/WASM Compilation
- Please don't add like 10MB Rust library or anything like that. We will eventually 'build' the whole thing to WASM/WASI and we don't want a like 50MB binary.
- PLEASE KEEP IN MIND SWC Tears down the WASI instance between CALLS. ANY Cross-transform caching will be destroyed; If you feel like you need cross-transform caching COMMUNICATE with me.

## Final bit of important info.
- CSS being byte-equal is non-negotaible; However is surrounding JS has byte-equal differnces that are PURELY cosmetic but the 'cure is worse than the disease' I.E to fix the issue it could be extremely complex/sacrifice perf goals. Then let's DISCUSS IT
- 