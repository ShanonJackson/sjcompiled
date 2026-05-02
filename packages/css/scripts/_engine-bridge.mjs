// Subprocess helper used by verify-engine-flag.mjs. Reads CSS from
// stdin, runs `sort()` from sort.ts (which honors COMPILED_CSS_ENGINE),
// writes the result to stdout. Lives in a separate file so we don't
// have to escape it through `bun -e`.

import { sort } from '../src/sort.ts';

let css = '';
process.stdin.setEncoding('utf8');
for await (const chunk of process.stdin) css += chunk;
process.stdout.write(sort(css));
