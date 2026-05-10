#!/usr/bin/env node
// Standalone reproduction for: SWC-native compiled-babel-plugin port
// SIGSEGVs when transforming a particular `*-compiled.tsx` file under
// `platform/packages/editor/editor-core/`.
//
// What this script does
// =====================
// Loads the offending source file, applies the same pragma-strip that
// the Jira jest-common babel-transformer.js applies, then runs it
// through @atlassian/swc-native with the same plugin options the
// production Jira pipeline uses. The process SIGSEGVs (no Node-level
// exception, no stack trace — the C++/Rust addon hard-crashes).
//
// Compare to:
//   * Engine 2 (stock @swc/core, no compiled plugin) — survives.
//   * Engine 1 (Babel + JS @compiled/babel-plugin oracle) — survives.
//
// This proves the crash is in the SWC compiled-babel-plugin port
// (or one of its transitive native deps such as oxc_resolver).
//
// Usage:
//   node platform/crates/sjcompiled/tmp_rovodev_repro_segv_editor_content_container/run.mjs [engine]
//
// engine ∈ { all | babel | swc | native }   (default: all)
//
// `all` runs them in order so you can see exactly which one dies.
// Switch to `native` alone for a clean SIGSEGV reproduction.

import { readFileSync, copyFileSync, existsSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const require = createRequire(import.meta.url);

const AFM_ROOT = resolve(__dirname, '../../../..');
const afmRequire = createRequire(resolve(AFM_ROOT, 'package.json'));

// Anchor cwd at the AFM root. The Jira `compiled-resolver` ultimately
// uses `@atlassian/resolver-core`, which (transitively) consults
// process.cwd() for some workspace lookups. Running this driver from a
// different cwd surfaces a spurious "Cannot find module
// '@atlaskit/tokens'" that has nothing to do with the SIGSEGV under
// investigation.
process.chdir(AFM_ROOT);

const SOURCE_FILE_REAL = resolve(
	AFM_ROOT,
	'platform/packages/editor/editor-core/src/ui/EditorContentContainer/EditorContentContainer-compiled.tsx',
);
// Cache a local copy alongside this driver so the repro is fully
// self-contained for the upstream sjcompiled team. Falls back to the
// real file in-tree when the local copy is missing.
const SOURCE_FILE_LOCAL = resolve(__dirname, 'input.tsx');
if (!existsSync(SOURCE_FILE_LOCAL) && existsSync(SOURCE_FILE_REAL)) {
	copyFileSync(SOURCE_FILE_REAL, SOURCE_FILE_LOCAL);
}
const SOURCE_FILE = existsSync(SOURCE_FILE_LOCAL)
	? SOURCE_FILE_LOCAL
	: SOURCE_FILE_REAL;
const SOURCE = readFileSync(SOURCE_FILE, 'utf8');

const banner = (label) =>
	`\n${'='.repeat(72)}\n${label}\n${'='.repeat(72)}\n`;

console.log(`SOURCE_FILE = ${SOURCE_FILE}`);
console.log(`SOURCE_LEN  = ${SOURCE.length} bytes (${SOURCE.split('\n').length} lines)`);

function stripJsxPragmas(src) {
	return src.replace(/@(jsx(?:Runtime|Frag|ImportSource)?)\b/g, '~$1');
}

// ─────────────────────────────────────────────────────────────────────
// Engine 1 — JS oracle
// ─────────────────────────────────────────────────────────────────────
function runBabelOracle() {
	const { transformSync } = afmRequire('@babel/core');
	const compiledBabelPlugin =
		afmRequire('@compiled/babel-plugin').default ??
		afmRequire('@compiled/babel-plugin');
	// Use the in-tree path so @compiled/babel-plugin's resolver can
	// walk up to the workspace `@atlaskit/tokens` package. Without
	// this we'd get a spurious "Cannot find module '@atlaskit/tokens'"
	// from a directory that has no node_modules.
	//
	// Plugin ordering MIRRORS the real Jira pipeline at
	// `jira/dev-tooling/packages/jest-common/src/babel-transformer.js:478-484`:
	//
	//   1. `@atlaskit/tokens/babel-plugin` runs FIRST. It rewrites
	//      every `token('...')` call into a static CSS string. Without
	//      this, `@compiled/babel-plugin` sees `token(...)` calls
	//      inside `cssMap({...})` values and reports them as
	//      "indirect selector + dynamic variable" violations — a
	//      genuine Compiled limitation, but ONE THAT THE REAL
	//      PIPELINE NEVER HITS because the calls have already been
	//      collapsed to literals by the previous plugin.
	//
	//   2. `@compiled/babel-plugin` runs second with the Jira
	//      production resolver
	//      (`@jira-dev/compiled-resolver` → `@atlassian/resolver-core`).
	//
	// Test this in isolation with `IS_SWC_NATIVE_ENABLED = false` in
	// `babel-transformer.js`: the failing test
	// (`AnnouncementBannerEditor.test.tsx`) PASSES via this exact
	// ordering. So the file IS valid Compiled input — the SWC port's
	// SIGSEGV is a real port-side bug, not "garbage in".
	const realResolver = afmRequire(
		'/home/ubuntu/atlassian-frontend-monorepo/jira/dev-tooling/packages/compiled-resolver/index.js'
	);
	const result = transformSync(SOURCE, {
		filename: SOURCE_FILE_REAL,
		babelrc: false,
		configFile: false,
		comments: false,
		compact: false,
		presets: [
			[afmRequire.resolve('@babel/preset-typescript'), {
				isTSX: true,
				allExtensions: true,
				onlyRemoveTypeImports: true,
			}],
			[afmRequire.resolve('@babel/preset-react'), {
				runtime: 'classic',
				useSpread: true,
			}],
		],
		plugins: [
			[afmRequire.resolve('@atlaskit/tokens/babel-plugin'), {
				shouldUseAutoFallback: true,
				shouldForceAutoFallback: false,
			}],
			[compiledBabelPlugin, {
				parserBabelPlugins: ['typescript', 'jsx'],
				resolver: realResolver,
			}],
		],
		parserOpts: { plugins: ['typescript', 'jsx'] },
	});
	return result?.code ?? '';
}

// ─────────────────────────────────────────────────────────────────────
// Engine 2 — Stock @swc/core, no compiled plugin
// ─────────────────────────────────────────────────────────────────────
function runStockSwc() {
	const swc = afmRequire('@swc/core');
	const stripped = stripJsxPragmas(SOURCE);
	const result = swc.transformSync(stripped, {
		filename: SOURCE_FILE,
		jsc: {
			parser: { syntax: 'typescript', tsx: true },
			target: 'es2022',
			transform: {
				verbatimModuleSyntax: true,
				react: {
					runtime: 'classic',
					pragma: 'React.createElement',
					pragmaFrag: 'React.Fragment',
				},
			},
			preserveAllComments: false,
		},
	});
	return result?.code ?? '';
}

// ─────────────────────────────────────────────────────────────────────
// Engine 3 — @atlassian/swc-native + compiled SWC port
// (mirrors babel-transformer.js exactly, minus the host snapshots
//  which are off-corpus for this crash investigation)
// ─────────────────────────────────────────────────────────────────────
function runSwcNative() {
	const swcNative = require(resolve(AFM_ROOT, 'platform/crates/swc-native'));
	const stripped = stripJsxPragmas(SOURCE);
	const out = swcNative.transformSync(stripped, {
		filename: SOURCE_FILE,
		jsc: {
			parser: { syntax: 'typescript', tsx: true },
			target: 'es2022',
			transform: {
				verbatimModuleSyntax: true,
				react: {
					runtime: 'classic',
					pragma: 'React.createElement',
					pragmaFrag: 'React.Fragment',
				},
			},
			preserveAllComments: false,
			experimental: {
				runPluginFirst: true,
				plugins: [
					[
						'@atlaskit/tokens',
						{
							shouldUseAutoFallback: true,
							shouldForceAutoFallback: false,
						},
					],
					[
						'@atlassian/swc-plugin-compiled',
						{
							addComponentName: true,
							extract: true,
							inlineCss: false,
							sortAtRules: true,
							sortShorthand: true,
							importSources: ['@atlaskit/css'],
							cache: false,
							resolver: {
								extensions: ['.ts', '.tsx', '.mjs', '.js', '.jsx', '.cjs', '.json'],
							},
						},
					],
				],
			},
		},
	});
	return out?.code ?? '';
}

const ENGINE = process.argv[2] ?? 'all';

if (ENGINE === 'all' || ENGINE === 'babel') {
	console.log(banner('ENGINE 1 — @compiled/babel-plugin (JS oracle)'));
	try {
		const code = runBabelOracle();
		console.log(`✅ survived. output length = ${code.length} bytes`);
	} catch (e) {
		console.error('❌ Babel oracle threw:', e.message);
	}
}

if (ENGINE === 'all' || ENGINE === 'swc') {
	console.log(banner('ENGINE 2 — stock @swc/core (no compiled plugin)'));
	try {
		const code = runStockSwc();
		console.log(`✅ survived. output length = ${code.length} bytes`);
	} catch (e) {
		console.error('❌ Stock SWC threw:', e.message);
	}
}

if (ENGINE === 'all' || ENGINE === 'native') {
	console.log(banner('ENGINE 3 — @atlassian/swc-native + compiled SWC port'));
	console.log('(if this is the last line printed, the process SIGSEGVd inside the addon)');
	try {
		const code = runSwcNative();
		console.log(`✅ survived. output length = ${code.length} bytes`);
	} catch (e) {
		console.error('❌ SWC-native threw:', e.message);
		console.error(e.stack);
	}
}

console.log(banner('DONE — all selected engines completed without SIGSEGV'));
