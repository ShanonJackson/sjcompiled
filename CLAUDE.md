## Goal

What we're doing is packages/css/src/transform.ts to Rust 1:1; What this means is that following PLAN.md and crates/PARITY_VERSIONS.md (describes which versions we need co copy) we need to replicate EVERYTHING 
1:1 in Rust (yes rebuild postcss in Rust) by 1:1 it means that all bugs, file folder structure for the original js or ts source needs to be migrated IDENTICALLY. The reason why all bugs and identically is that at the end
of that pipeline a hash is generated; The hashes for all INPUTS needs to remain identical. For the hash to remain identical any 'white space' or parsing/string manipuation obviously needs to be identical in the new Rust copy.
When we're finished we will call out to Rust via NAPI synchronously in that file so that from ANY consumers perspective there is NO percievable difference in OUTPUT EVER UNDER ANY CIRCUMSTANCE.

This is obviously incredibly complex work, The only way the "WHOLE" is what we want is if all the "PARTS" are 1:1; Which means we'll need to rigorously test all the parts as we go

