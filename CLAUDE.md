## Goal

What we're doing is packages/css/src/transform.ts to Rust 1:1; What this means is that following PLAN.md and crates/PARITY_VERSIONS.md (describes which versions we need co copy) we need to replicate EVERYTHING 
1:1 in Rust (yes rebuild postcss in Rust) by 1:1 it means that all bugs, file folder structure for the original js or ts source needs to be migrated IDENTICALLY. The reason why all bugs and identically is that at the end
of that pipeline a hash is generated; The hashes for all INPUTS needs to remain identical. For the hash to remain identical any 'white space' or parsing/string manipuation obviously needs to be identical in the new Rust copy.
When we're finished we will call out to Rust via NAPI synchronously in that file so that from ANY consumers perspective there is NO percievable difference in OUTPUT EVER UNDER ANY CIRCUMSTANCE.

This is obviously incredibly complex work, The only way the "WHOLE" is what we want is if all the "PARTS" are 1:1; Which means we'll need to rigorously test all the parts as we go

In order to do this correctly, it's probably best if we replicate their folder/file structure of what we have to port from JS -> Rust identically as well. That way it's very easy to compare old/new source and spot differences in logic (which should never occur)


# DRIFT DETECTION - 
THIS PART IS CRITICAL!
If you think someone hasn't ported something OUTSIDE your work CORRECTLY; Immedietly I.E "Drift detected in X - <Explanation here>" this is CRITICALLY important. otherwise if many things have slight drift the "WHOLE" will have MAJOR drift. Minor drift is unnacceptable.
DONT try and "WORK AROUND" drift; That's not your call to make. Drift is the enemy.

# Quality
Quality is more important than speed; Again if we port something even slightly incorrectly when we eventually integrate it into the 60-90GB Monorepo ANY issue that CAN happen WILL happen.


# Performance
Performance is a side-goal that should NEVER come at the cost of correctness but yet is important. If we ship the entire thing end-to-end and it's slower than the original we've failed.
