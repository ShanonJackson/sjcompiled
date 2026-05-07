#!/usr/bin/env bash
# Bisect a single divergent fixture down to the first pipeline stage
# whose JS-vs-Rust output diverges.
#
# Usage:
#   parity-runner/scripts/bisect.sh <fixture.css>
#
# Walks every parity-runner stage in transform-css pipeline order; for
# each stage runs `parity-runner --stage <s> --corpus /tmp/<single>` on
# just this fixture. Prints the first stage that fails and exits with
# its exit code; exits 0 if every stage is byte-clean (i.e. the
# divergence is at `transform-css` assembly only — class-name hashing
# or sheets aggregation rather than any individual plugin).
#
# Run from the `crates/` directory.

set -u

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <fixture.css>" >&2
    exit 2
fi

FIXTURE="$1"
if [[ ! -f "$FIXTURE" ]]; then
    echo "not a file: $FIXTURE" >&2
    exit 2
fi

# Pipeline order, mirroring packages/css/src/transform.ts.
# `postcss-core-roundtrip` first as the parser/stringifier sanity gate.
# Then the local plugins in transform.ts order, then the cssnano
# sub-plugins (only meaningful as a band on full pipelines, but each
# diffs in isolation here too), then autoprefixer, then the assembled
# transform-css gate as the final tie-breaker.
STAGES=(
    postcss-core-roundtrip
    atomicify-rules
    parent-orphaned-pseudos
    flatten-multiple-selectors
    expand-shorthands
    postcss-nested
    normalize-current-color
    discard-empty-rules
    discard-duplicates
    merge-duplicate-at-rules
    sort-atomic-style-sheet
    increase-specificity
    extract-stylesheets
    postcss-discard-comments
    postcss-normalize-string
    postcss-normalize-positions
    postcss-normalize-timing-functions
    postcss-normalize-url
    postcss-normalize-unicode
    postcss-minify-selectors
    postcss-minify-params
    postcss-ordered-values
    postcss-reduce-initial
    postcss-colormin
    postcss-minify-gradients
    postcss-calc
    postcss-convert-values
    postcss-normalize-whitespace
    npm-postcss-discard-duplicates
    cssnano-band
    autoprefixer
    transform-css
)

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
cp "$FIXTURE" "$TMPDIR/x.css"

NAME=$(basename "$FIXTURE")
for stage in "${STAGES[@]}"; do
    OUT=$(./target/debug/parity-runner --stage "$stage" --corpus "$TMPDIR" 2>&1)
    RC=$?
    if [[ $RC -ne 0 ]]; then
        # Skip stages this binary doesn't know about (planned but not yet
        # wired). Treat exit 2 (setup error: "unknown stage: ...") as
        # not-applicable.
        if [[ "$OUT" == *"unknown stage:"* ]]; then
            continue
        fi
        echo "=== $NAME → first divergence at stage: $stage ==="
        echo "$OUT"
        exit 1
    fi
done

echo "$NAME: every stage byte-clean (transform-css-only divergence)"
exit 0
