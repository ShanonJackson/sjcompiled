## Versions of each sub-package and the commit it comes from:
@compiled/babel-plugin 0.36.1 16a62b8 (solo patch release)
@compiled/babel-plugin-strip-runtime 0.36.0 40a4548 (Jan 28 batch)
@compiled/css 0.19.0 40a4548 (used by BOTH above, nested)
@compiled/utils 0.13.2 130ed3b (hoisted, shared)

## Versions of all the transitive dependencies and what they're set to:



Complete @compiled/css@0.19.0 dependency tree — exact installed versions
Direct deps of @compiled/css@0.19.0

Package
Version
Notes
@compiled/utils
0.13.2
hoisted
autoprefixer
10.4.14
hoisted
cssnano-preset-default
5.2.14
hoisted
postcss
8.5.6
hoisted
postcss-discard-duplicates
6.0.0
hoisted
postcss-nested
5.0.6
hoisted
postcss-normalize-whitespace
5.1.1
hoisted
postcss-selector-parser
6.1.2
hoisted
postcss-values-parser
6.0.2
hoisted
cssnano-preset-default@5.2.14 sub-plugins

Package
Version
Notes
css-declaration-sorter
6.3.1

cssnano-utils
3.1.0

postcss-calc
8.2.4

postcss-colormin
5.3.1

postcss-convert-values
5.1.3

postcss-discard-comments
5.1.2

postcss-discard-duplicates
5.1.0
⚠️ NESTED under cssnano — different from hoisted 6.0.0
postcss-discard-empty
5.1.1

postcss-discard-overridden
5.1.0

postcss-merge-longhand
5.1.7

postcss-merge-rules
5.1.4

postcss-minify-font-values
5.1.0

postcss-minify-gradients
5.1.1

postcss-minify-params
5.1.4

postcss-minify-selectors
5.2.1

postcss-normalize-charset
5.1.0

postcss-normalize-display-values
5.1.0

postcss-normalize-positions
5.1.1

postcss-normalize-repeat-style
5.1.1

postcss-normalize-string
5.1.0

postcss-normalize-timing-functions
5.1.0

postcss-normalize-unicode
5.1.1

postcss-normalize-url
5.1.0

postcss-normalize-whitespace
5.1.1

postcss-ordered-values
5.1.3

postcss-reduce-initial
5.1.2

postcss-reduce-transforms
5.1.0

postcss-svgo
5.1.0

postcss-unique-selectors
5.1.1

autoprefixer@10.4.14 deps

Package
Version
browserslist
4.24.2
caniuse-lite
1.0.30001766
fraction.js
4.2.0
normalize-range
0.1.2
picocolors
1.1.1
postcss-value-parser
4.2.0
svgo@3.3.2 deps (via postcss-svgo)

Package
Version
Notes
@trysound/sax
0.2.0

commander
7.2.0
⚠️ NESTED under svgo — hoisted is 8.3.0
css-select
5.1.0

css-tree
2.3.1
⚠️ NESTED under svgo — hoisted is 1.1.3
css-what
6.1.0

csso
5.0.5
⚠️ NESTED under svgo — hoisted csso is 4.2.0
mdn-data
2.0.30
⚠️ NESTED under svgo's css-tree — hoisted is 2.0.14
picocolors
1.1.1

caniuse-api@3.0.0 deps (via postcss-merge-rules, postcss-reduce-initial)

Package
Version
browserslist
4.24.2
caniuse-lite
1.0.30001766
lodash.memoize
4.1.2
lodash.uniq
4.5.0
css-select@5.1.0 deps (used by svgo)

Package
Version
Notes
boolbase
1.0.0

css-what
6.1.0

domhandler
5.0.3
⚠️ NESTED under css-select
domutils
3.2.2
⚠️ NESTED under css-select
dom-serializer
2.0.0
⚠️ NESTED under css-select
entities
4.5.0
⚠️ NESTED under css-select
nth-check
2.1.1

csso@4.2.0 + css-tree@1.1.3 (hoisted, used by colord path)

Package
Version
Notes
csso
4.2.0
hoisted
css-tree
1.1.3
hoisted
mdn-data
2.0.14
hoisted
colord
2.9.3
(used by postcss-colormin)
cssesc
3.0.0

source-map
0.6.1


⚠️ The critical gotcha for your Rust port
Several packages exist in multiple versions simultaneously due to yarn hoisting conflicts. The ones that directly affect CSS output are:

1.  postcss-discard-duplicates: 5.1.0 runs inside cssnano, 6.0.0 runs as a standalone pass from @compiled/css directly — both run, in sequence
2.  css-tree: 1.1.3 (hoisted, used by csso@4.2.0) vs 2.3.1 (nested under svgo, used by csso@5.0.5) — svgo uses the newer one
3.  mdn-data: 2.0.14 (hoisted) vs 2.0.28/2.0.30 (nested under svgo's css-tree) — affects SVG/CSS property data
    For your Rust reimplementation, the SVG-related stack (svgo, csso, css-tree) is probably the lowest priority — it only matters if Jira's compiled CSS contains inline SVGs in CSS properties, which is rare. The highest-priority byte-critical items are the CSS class name hash function in @compiled/utils and the atomic CSS generation in @compiled/css itself.
    Would you like me to drill into exactly what @compiled/utils@0.13.2 does for class name hashing — that's the single most critical function to get byte-identical?

