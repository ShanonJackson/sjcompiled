#!/usr/bin/env node
// Try to construct an even-smaller synthetic input that still SIGSEGVs.
// Hypothesis: the trigger is a `cssMap.<member>` chain inside a JSX
// className-array that has many members + ternaries + spreads.
//
// We progressively expand a synthetic component and find the smallest
// one that crashes.

import { writeFileSync, readFileSync, existsSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const INPUT_FILE = join(__dirname, 'input.tsx');
const ORIGINAL_INPUT = existsSync(INPUT_FILE) ? readFileSync(INPUT_FILE) : null;

const HEADER = `/**
 * @jsxRuntime classic
 * @jsx jsx
 */
import React from 'react';
import { css, cssMap, jsx, keyframes } from '@compiled/react';

const styles = cssMap({
\tbase: { color: 'red' },
});

`;

function build(numMemberAccesses, withTernaries, withSpreads, withForwardRef) {
	let body = '';
	for (let i = 0; i < numMemberAccesses; i++) {
		body += `\t\tstyles.base,\n`;
	}
	if (withTernaries) {
		for (let i = 0; i < 5; i++) {
			body += `\t\ttrue ? styles.base : styles.base,\n`;
		}
	}
	if (withSpreads) {
		body += `\t\ttrue && [\n`;
		for (let i = 0; i < 10; i++) body += `\t\t\tstyles.base,\n`;
		body += `\t\t],\n`;
	}
	const compRef = withForwardRef
		? `React.forwardRef<HTMLDivElement, {}>((props, ref) => (\n\t<div ref={ref} className={[\n${body}\t]}>{props.children}</div>\n))`
		: `(props: any) => (\n\t<div className={[\n${body}\t]}>{props.children}</div>\n)`;
	return HEADER + `export const Foo = ${compRef};\n`;
}

const tries = [
	{ label: 'plain fn, 5 members',           src: build(5, false, false, false) },
	{ label: 'plain fn, 50 members',          src: build(50, false, false, false) },
	{ label: 'plain fn, 120 members',         src: build(120, false, false, false) },
	{ label: 'forwardRef, 5 members',         src: build(5, false, false, true) },
	{ label: 'forwardRef, 50 members',        src: build(50, false, false, true) },
	{ label: 'forwardRef, 120 members',       src: build(120, false, false, true) },
	{ label: 'forwardRef, 120 + ternaries',   src: build(120, true, false, true) },
	{ label: 'forwardRef, 120 + spreads',     src: build(120, false, true, true) },
	{ label: 'forwardRef, 120 + ter + sprd',  src: build(120, true, true, true) },
	{ label: 'forwardRef, 200 + ter + sprd',  src: build(200, true, true, true) },
];

for (const t of tries) {
	writeFileSync(INPUT_FILE, t.src);
	const result = spawnSync('node', [join(__dirname, 'run.mjs'), 'native'], {
		stdio: ['ignore', 'ignore', 'ignore'],
		timeout: 60_000,
	});
	const segv = result.signal === 'SIGSEGV' || (result.status !== null && result.status > 128);
	console.log(`${t.label.padEnd(40)}  ${t.src.split('\n').length} lines, ${t.src.length} bytes  → ${segv ? '❌ SIGSEGV' : '✅ ok'}`);
}

// Restore.
if (ORIGINAL_INPUT) writeFileSync(INPUT_FILE, ORIGINAL_INPUT);
console.log('\n(input.tsx restored)');
