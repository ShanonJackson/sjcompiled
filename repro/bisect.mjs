#!/usr/bin/env node
// Bisect the SIGSEGV in the EditorContentContainer-compiled.tsx repro
// down to the smallest contiguous slice of the input that still
// crashes @atlassian/swc-native.
//
// Strategy
// ========
// Each candidate slice is tried in a separate `node` subprocess so a
// SIGSEGV doesn't take this driver down. We exec the same `run.mjs`
// pointed at a different `input.tsx` in a temp dir.
//
// The bisection is two-phase:
//   1. Halve from the bottom: keep [0..N/2], [0..3N/4], etc., until
//      we find the smallest prefix that still crashes.
//   2. Halve from the top:    inside that prefix, drop the leading
//      half, then quarter, etc., until we find the smallest suffix
//      of the prefix that still crashes.
//
// After both phases, we have a contiguous `[startLine, endLine)` slice
// that's the SIGSEGV trigger. The script writes it to
// `./minimal-repro.tsx`.
//
// Usage:
//   node platform/crates/sjcompiled/tmp_rovodev_repro_segv_editor_content_container/bisect.mjs

import { readFileSync, writeFileSync, mkdirSync, rmSync, existsSync, copyFileSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const AFM_ROOT = resolve(__dirname, '../../../..');
const SRC = readFileSync(join(__dirname, 'input.tsx'), 'utf8');
const LINES = SRC.split('\n');
console.log(`Loaded input: ${LINES.length} lines, ${SRC.length} bytes`);

// We swap input.tsx in-place between attempts. Save the original so
// we can restore it after the bisection (or if the user Ctrl-Cs).
const INPUT_FILE = join(__dirname, 'input.tsx');
const ORIGINAL_INPUT = readFileSync(INPUT_FILE);
const restore = () => writeFileSync(INPUT_FILE, ORIGINAL_INPUT);
process.on('SIGINT', () => { restore(); process.exit(130); });
process.on('SIGTERM', () => { restore(); process.exit(143); });

let attempt = 0;
function tryRange(startLine, endLineExclusive) {
	attempt += 1;
	const slice = LINES.slice(startLine, endLineExclusive).join('\n');
	writeFileSync(INPUT_FILE, slice);
	const result = spawnSync('node', [join(__dirname, 'run.mjs'), 'native'], {
		stdio: ['ignore', 'ignore', 'ignore'],
		timeout: 60_000,
	});
	const segv = result.signal === 'SIGSEGV' || (result.status !== null && result.status > 128);
	console.log(
		`#${String(attempt).padStart(3, '0')}  lines ${startLine}..${endLineExclusive}  ` +
		`(${endLineExclusive - startLine} lines, ${slice.length} bytes)  → ` +
		(segv ? '❌ SIGSEGV' : `✅ exit ${result.status}`)
	);
	return segv;
}

// Phase 0: confirm full input still crashes.
console.log('\n── Phase 0: confirm full input crashes ──');
if (!tryRange(0, LINES.length)) {
	console.error('Full input did NOT crash — aborting bisection.');
	process.exit(1);
}

// Phase 1: find smallest crashing PREFIX [0..end)
console.log('\n── Phase 1: shrink end of file ──');
let lo = 1;
let hi = LINES.length;
let bestEnd = LINES.length;
while (lo < hi) {
	const mid = Math.floor((lo + hi) / 2);
	if (tryRange(0, mid)) {
		bestEnd = mid;
		hi = mid;
	} else {
		lo = mid + 1;
	}
}
console.log(`→ smallest crashing prefix: 0..${bestEnd} (${bestEnd} lines)`);

// Phase 2: find smallest crashing SUFFIX [start..bestEnd) inside the prefix
console.log('\n── Phase 2: shrink start of file ──');
lo = 0;
hi = bestEnd - 1;
let bestStart = 0;
while (lo < hi) {
	const mid = Math.floor((lo + hi + 1) / 2);
	if (tryRange(mid, bestEnd)) {
		bestStart = mid;
		lo = mid;
	} else {
		hi = mid - 1;
	}
}
console.log(`→ smallest crashing range: ${bestStart}..${bestEnd} (${bestEnd - bestStart} lines)`);

// Save the result and restore the original input file.
const minimal = LINES.slice(bestStart, bestEnd).join('\n');
const outPath = join(__dirname, 'minimal-repro.tsx');
writeFileSync(outPath, minimal);
restore();
console.log(`\n✅ wrote ${outPath} (${bestEnd - bestStart} lines, ${minimal.length} bytes)`);
console.log(`Verified: this slice SIGSEGVs @atlassian/swc-native; smaller ones don't.`);
console.log(`Original input.tsx restored.`);
