// What does JS cross-fade.replace do for the canonical AFM input?
const args = `url('a.png'), url('b.png'), 50%`;
const m = args.match(/\d*.?\d+%?/);
console.log('match:', JSON.stringify(m && m[0]), 'index:', m && m.index, 'length:', m && m[0].length);
const sliced = args.slice(m[0].length);
console.log('args.slice(length):', JSON.stringify(sliced));
const trimmed = sliced.trim();
console.log('trimmed:', JSON.stringify(trimmed));
const final = trimmed + `, ${m[0]}`;
console.log('final args:', JSON.stringify(final));
console.log('full result:', `-webkit-cross-fade(${final})`);
