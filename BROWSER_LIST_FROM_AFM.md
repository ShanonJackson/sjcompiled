Actual Jira build command path
jira/package.json:

build: dev-tooling/cli/bin/run build
dev-tooling/cli/src/commands/build.ts loads the selected variant and calls:

const command = await variant.build(flags, argv.join(' '));
runCommand(command);
runCommand() uses spawnSync() with no cwd override, so the child process inherits the current working directory of npm run build.
For Jira’s build, that is expected to be:

/home/ubuntu/atlassian-frontend-monorepo/jira
The default variant builds the command:

BABEL_ENV=production
BUILD_VARIANT=default
node ./node_modules/.bin/atlaspack build
--dist-dir build/assets
--target default
--public-url ${PUBLIC_PATH}assets/
--detailed-report 0
--no-autoinstall
--watch-backend watchman
--config .parcelrc-v3
...
Important nuance: NODE_ENV=production is only explicitly added by this variant for --local or if options.NODE_ENV is set. But for Browserslist here that does not matter because Jira’s .browserslistrc has only default entries, no [production] / [development] sections.
Actual transformer chain in build
.parcelrc-v3 has:

"*.{js,mjs,jsm,jsx,es6,cjs,ts,tsx}": [
"@atlaspack/transformer-tokens",
"@atlaspack/transformer-compiled-css-in-js",
"@atlassian/parcel-transformer-inline-bundle-splittable",
"@compiled/parcel-transformer-external",
"@compiled/parcel-transformer",
...
]
The legacy JS Compiled path you’re asking about is:

@compiled/parcel-transformer@0.18.4
-> @compiled/babel-plugin@0.36.1
-> nested @compiled/css@0.19.0
-> autoprefixer@10.4.14
-> browserslist@4.24.2
The exact nested @compiled/css is:

jira/node_modules/@compiled/babel-plugin/node_modules/@compiled/css
not:

jira/node_modules/@compiled/css
Exact .browserslistrc used by the build-stage Autoprefixer
Because the actual build command runs atlaspack build from Jira root, and because @compiled/css@0.19.0 calls:

autoprefixer()
with no override, Autoprefixer resolves Browserslist relative to the build process cwd:

/home/ubuntu/atlassian-frontend-monorepo/jira
So the build-stage config is:

/home/ubuntu/atlassian-frontend-monorepo/jira/.browserslistrc
Exact contents:

# Browsers that we support
# This is used by [eslint-plugin-compat](https://github.com/amilajack/eslint-plugin-compat)
# to automatically detected browser compatibility issues.

# [Supported browser matrix](https://confluence.atlassian.com/cloud/supported-browsers-744721663.html)
# [Format for browserslist](https://github.com/ai/browserslist)

# DO NOT use last 1 <browsername> to avoid accidentally dropping browser support, once a new version is released

# no environment specified - used as the default
last 2 Edge version
last 2 Firefox version
last 5 Chrome version # we have 5 recent versions of chrome among top 10 most used browsers
last 2 Safari version
last 2 iOS version
last 2 ChromeAndroid version
File SHA-256:

08c8e1bf56ad773621c9b264971365f66f78a808d6d369a4ea9584a02da459cb
Exact resolved browser targets for Jira build’s Compiled CSS path
For the actual build-stage path:

atlaspack build from jira/
-> @compiled/parcel-transformer
-> @compiled/babel-plugin
-> @compiled/css@0.19.0
-> autoprefixer()
the effective resolved list is:

and_chr 144
chrome 144
chrome 143
chrome 142
chrome 141
chrome 140
edge 144
edge 143
firefox 147
firefox 146
ios_saf 26.2
ios_saf 26.1
safari 26.2
safari 26.1
No and_chr 143.
What agents should reproduce
For byte-equal rebuild work, tell them:

Run/build context:
- process cwd: jira/
- build command: atlaspack build --config .parcelrc-v3 ...
- BABEL_ENV=production
- BUILD_VARIANT=default
- BROWSERSLIST_ENV unset unless build explicitly sets it
- AUTOPREFIXER not "off"  

Compiled package path:
- @compiled/parcel-transformer 0.18.4
- @compiled/babel-plugin 0.36.1
- @compiled/css 0.19.0, nested under @compiled/babel-plugin/node_modules

Autoprefixer/Browserslist:
- autoprefixer 10.4.14
- browserslist 4.24.2
- caniuse-lite 1.0.30001766
- config file: jira/.browserslistrc
- do not use mobileToDesktop: true
  I could not get a full Atlaspack unstable_transform verifier to complete due an Atlaspack V3 API validation issue, but the actual build command, cwd inheritance, .parcelrc-v3 transformer chain, nested package resolution, and @compiled/css@0.19.0 transform implementation were verified directly. Should I next produce a single “build parity contract” block with all versions/configs/options for the SWC agents?