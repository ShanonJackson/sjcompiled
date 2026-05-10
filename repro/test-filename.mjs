#!/usr/bin/env node
// Test whether the SIGSEGV depends on the `filename` value passed to
// swc-native. The babel-transformer comment block at lines 421-428
// blames `host_to_wasi` path-translation for the segfault, but
// `apply_native` doc says native callers pass `opts.root = None` so
// `host_to_wasi` is a no-op. If the filename influences the crash,
// it implicates the resolver consuming `filename` directly.

import { readFileSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));

const filenames = [
	// Real path inside the AFM monorepo (the original report site)
	resolve(__dirname, '../../../../platform/packages/editor/editor-core/src/ui/EditorContentContainer/EditorContentContainer-compiled.tsx'),
	// Local cache of the same content beside the repro driver
	resolve(__dirname, 'minimal-repro-2.tsx'),
	// Non-existent path
	'/tmp/doesnotexist.tsx',
	// `<anonymous>`-style sentinel
	'<anon>',
	// Empty
	'',
];

for (const filename of filenames) {
	const result = spawnSync('node', [
		'-e',
		`
			const { resolve } = require('path');
			const { readFileSync } = require('fs');
			const swc = require(resolve(process.cwd(), 'platform/crates/swc-native'));
			const src = readFileSync(${JSON.stringify(resolve(__dirname, 'minimal-repro-2.tsx'))}, 'utf8');
			const stripped = src.replace(/@(jsx(?:Runtime|Frag|ImportSource)?)\\b/g, '~$1');
			const out = swc.transformSync(stripped, {
				filename: ${JSON.stringify(filename)},
				jsc: {
					parser: { syntax: 'typescript', tsx: true },
					target: 'es2022',
					transform: {
						verbatimModuleSyntax: true,
						react: { runtime: 'classic', pragma: 'React.createElement', pragmaFrag: 'React.Fragment' },
					},
					preserveAllComments: false,
					experimental: {
						runPluginFirst: true,
						plugins: [
							['@atlaskit/tokens', { shouldUseAutoFallback: true, shouldForceAutoFallback: false }],
							['@atlassian/swc-plugin-compiled', {
								addComponentName: true,
								extract: true,
								inlineCss: false,
								sortAtRules: true,
								sortShorthand: true,
								importSources: ['@atlaskit/css'],
								cache: false,
								resolver: { extensions: ['.ts', '.tsx', '.mjs', '.js', '.jsx', '.cjs', '.json'] },
							}],
						],
					},
				},
			});
			console.log('✅ output bytes:', out.code.length);
		`,
	], {
		cwd: resolve(__dirname, '../../../..'),
		stdio: ['ignore', 'pipe', 'pipe'],
		timeout: 60_000,
	});
	const segv = result.signal === 'SIGSEGV' || (result.status !== null && result.status > 128);
	const status = segv ? '❌ SIGSEGV' : `exit ${result.status}`;
	console.log(`filename=${JSON.stringify(filename).padEnd(120)}  → ${status}`);
}
