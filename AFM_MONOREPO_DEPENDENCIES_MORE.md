
Complete @compiled/css@0.19.0 dependency manifest — all 61 items
Items 60–61 first (the anchors)

#
Package
Resolved Version
Integrity / Confirmation
60
@compiled/css
0.19.0
checksum: 10/49af632abf6082f1f54b8c672f55c43cb4f2563902f27a47d105cc5895c89880599cca4eeb6b472029ac05bd08e0b885616ed46c23f8e0b6e2f4e9f8035e5385 · tarball via Atlassian private registry (npm-remote proxy of npmjs.org) — public equivalent: https://registry.npmjs.org/@compiled/css/-/css-0.19.0.tgz · gitHead confirmed as 40a45489eaaacc023110c3f107d702a389232892
61
@compiled/utils
0.13.2
gitHead confirmed 130ed3b4ae8a48926892939679c2f1479375f2a8

Direct deps of @compiled/css@0.19.0

#
Package
Resolved Version
Location
1
autoprefixer
10.4.14
hoisted
2
cssnano-preset-default
5.2.14
hoisted
3
postcss
8.5.6
hoisted
4
postcss-discard-duplicates
6.0.0
hoisted — this is the one @compiled/css calls directly
5
postcss-nested
5.0.6
hoisted
6
postcss-normalize-whitespace
5.1.1
hoisted
7
postcss-selector-parser
6.1.2
hoisted
8
postcss-values-parser
6.0.2
hoisted (plural — distinct from postcss-value-parser)
9
@compiled/utils
0.13.2
hoisted

Autoprefixer transitive deps

#
Package
Resolved Version
Location
10
browserslist
4.24.2
hoisted
11
caniuse-lite
1.0.30001766
hoisted
12
fraction.js
4.2.0
hoisted
13
normalize-range
0.1.2
hoisted
14
picocolors
1.1.1
hoisted
15
postcss-value-parser
4.2.0
hoisted (singular — distinct from postcss-values-parser)

cssnano-preset-default@5.2.14 subtree

#
Package
Resolved Version
Location
16
css-declaration-sorter
6.3.1
hoisted
17
cssnano-utils
3.1.0
hoisted
18
postcss-calc
8.2.4
hoisted
19
postcss-colormin
5.3.1
hoisted
20
postcss-convert-values
5.1.3
hoisted
21
postcss-discard-comments
5.1.2
hoisted
22
postcss-discard-duplicates
5.1.0
⚠️ NESTED: cssnano-preset-default/node_modules/ — different from #4's 6.0.0 — both run
23
postcss-discard-empty
5.1.1
hoisted
24
postcss-discard-overridden
5.1.0
hoisted
25
postcss-merge-longhand
5.1.7
hoisted
26
postcss-merge-rules
5.1.4
hoisted
27
postcss-minify-font-values
5.1.0
hoisted
28
postcss-minify-gradients
5.1.1
hoisted
29
postcss-minify-params
5.1.4
hoisted
30
postcss-minify-selectors
5.2.1
hoisted
31
postcss-normalize-charset
5.1.0
hoisted
32
postcss-normalize-display-values
5.1.0
hoisted
33
postcss-normalize-positions
5.1.1
hoisted
34
postcss-normalize-repeat-style
5.1.1
hoisted
35
postcss-normalize-string
5.1.0
hoisted
36
postcss-normalize-timing-functions
5.1.0
hoisted
37
postcss-normalize-unicode
5.1.1
hoisted
38
postcss-normalize-url
5.1.0
hoisted
39
postcss-ordered-values
5.1.3
hoisted
40
postcss-reduce-initial
5.1.2
hoisted
41
postcss-reduce-transforms
5.1.0
hoisted
42
postcss-svgo
5.1.0
hoisted
43
postcss-unique-selectors
5.1.1
hoisted

Caniuse data sources

#
Package
Resolved Version
Location
44
caniuse-api
3.0.0
hoisted
45
electron-to-chromium
1.5.41
hoisted
46
node-releases
2.0.18
hoisted
47
update-browserslist-db
1.1.1
hoisted

Color/value helpers

#
Package
Resolved Version
Location
48
colord
2.9.3
hoisted
49
cssesc
3.0.0
hoisted
50
source-map
0.6.1
hoisted

SVGO subtree (via postcss-svgo)
⚠️ Multiple versions of the same package exist simultaneously. Each row that shows two versions means both are live in memory at runtime — one in each package's closure.

#
Package
Resolved Version
Location
51
svgo
3.3.2
hoisted
52
@trysound/sax
0.2.0
hoisted
53
commander
7.2.0
⚠️ NESTED: svgo/node_modules/
53b
commander
8.3.0
hoisted (used by everything else)
54
css-select
5.1.0
hoisted
55a
css-tree
1.1.3
hoisted — used by csso@4.2.0
55b
css-tree
2.3.1
⚠️ NESTED: svgo/node_modules/ — used by csso@5.0.5 inside svgo
56
css-what
6.1.0
hoisted
57a
csso
4.2.0
hoisted
57b
csso
5.0.5
⚠️ NESTED: svgo/node_modules/
58a
mdn-data
2.0.14
hoisted — used by css-tree@1.1.3
58b
mdn-data
2.0.30
⚠️ NESTED: svgo/node_modules/mdn-data/ — used by css-tree@2.3.1 inside svgo
59a
domhandler
4.3.1
hoisted — used by domutils@2.8.0
59b
domhandler
5.0.3
⚠️ NESTED: css-select/node_modules/
59c
domutils
2.8.0
hoisted
59d
domutils
3.2.2
⚠️ NESTED: css-select/node_modules/
59e
dom-serializer
0.1.1
hoisted
59f
dom-serializer
2.0.0
⚠️ NESTED: css-select/node_modules/
59g
entities
1.1.2
hoisted
59h
entities
4.5.0
⚠️ NESTED: css-select/node_modules/
59i
entities
2.2.0
⚠️ NESTED: domutils/node_modules/
59j
nth-check
2.1.1
hoisted
59k
boolbase
1.0.0
hoisted
59l
lodash.memoize
4.1.2
hoisted (via caniuse-api)
59m
lodash.uniq
4.5.0
hoisted (via caniuse-api)

Summary of ⚠️ byte-killer duplicates
These are the packages where two different versions run simultaneously — your Rust port must implement the correct version for each call site:

Package
Version A (hoisted)
Version B (nested, where)
postcss-discard-duplicates
6.0.0 (called by @compiled/css directly)
5.1.0 (called by cssnano-preset-default)
css-tree
1.1.3 (used by hoisted csso@4.2.0)
2.3.1 (used by svgo's csso@5.0.5)
csso
4.2.0
5.0.5 (inside svgo)
mdn-data
2.0.14
2.0.30 (inside svgo)
commander
8.3.0
7.2.0 (inside svgo)
domhandler
4.3.1
5.0.3 (inside css-select)
domutils
2.8.0
3.2.2 (inside css-select)
dom-serializer
0.1.1
2.0.0 (inside css-select)
entities
1.1.2
4.5.0 (css-select) · 2.2.0 (domutils)