// Regenerates `oracle.json` — the JS-vs-Rust parity corpus consumed by
// `crates/autoprefixer/tests/fraction_js_parity.rs`. Run from the
// workspace root:
//
//     node crates/autoprefixer/tests/fraction_js/gen_oracle.cjs
//
// The corpus covers every public method the folded-in `fraction.js`
// port (`crates/autoprefixer/src/fraction_js/`) exposes,
// including the autoprefixer-shaped `f.mul(2.54).div(96).simplify()`
// chain for the 8 dpcm media-query base values
// (72, 96, 120, 144, 192, 240, 288, 384) and the matching dpi chain.
//
// The test harness on the Rust side asserts byte-equal output for
// every fraction's `s`, `n`, `d`, `toFraction(false)`, `toFraction(true)`,
// `toString()`, and `valueOf()` (with NaN-as-string sentinel handling).

const path = require('path');
const fs = require('fs');
const Fraction = require(path.resolve(process.cwd(), 'crates/_vendor/fraction.js-4.2.0/package/fraction.js'));

const cases = [];
function record(label, op, fn) {
  let result;
  try {
    const r = fn();
    if (r === null || r === undefined) result = { kind: 'null' };
    else if (typeof r === 'boolean') result = { kind: 'bool', value: r };
    else if (typeof r === 'number') result = { kind: 'number', value: Number.isNaN(r) ? 'NaN' : String(r) };
    else if (typeof r === 'string') result = { kind: 'string', value: r };
    else if (Array.isArray(r)) result = { kind: 'array_of_number', value: r.map(v => Number.isNaN(v) ? 'NaN' : v) };
    else result = {
      kind: 'fraction',
      // s/n/d are stored as numbers because they are always integers in
      // valid Fraction state (post-gcd-reduction). JSON.stringify of an
      // integer in safe-int range round-trips exactly under serde_json.
      s: Number.isNaN(r.s) ? 'NaN' : r.s,
      n: Number.isNaN(r.n) ? 'NaN' : r.n,
      d: Number.isNaN(r.d) ? 'NaN' : r.d,
      toFraction: r.toFraction(),
      toFractionExcludeWhole: r.toFraction(true),
      toString: r.toString(),
      // `valueOf` is `s * n / d` — an arbitrary f64. serde_json's number
      // parser is not bit-accurate for full-precision decimals (it can
      // round 1 ULP off for values like `0.026458333333333334`), so we
      // store as a string and let Rust's `str::parse::<f64>()` round
      // the decimal to the nearest f64 (which IS bit-accurate).
      valueOf: Number.isNaN(r.valueOf()) ? 'NaN' : String(r.valueOf()),
    };
  } catch (e) {
    result = { kind: 'throw', message: e.message };
  }
  cases.push({ label, op, result });
}

const dpcmInputs = [72, 96, 120, 144, 192, 240, 288, 384];
const numericInputs = [0, 1, -1, 2, 3, 0.5, 1.5, 2.54, 96, 100, 1000, 1/3, -7/3, 0.1, 0.2];
const stringInputs = ['0', '1', '-1', '7/8', '-7/3', '1/3', '0.5', '-.5', '.5', '1.(3)', '0.(3)', '-0.(3)', "1.'3'", '1 1/2', '-2 1/3', '5:3', '1.0', '-0', '-0.0', '-', '+', '7'];
const pairInputs   = [[1,2], [3,4], [-1,2], [127,50], [127,4800], [254,9600], [0,1]];

for (const v of numericInputs) record(`new(${v})`, 'new_number', () => new Fraction(v));
for (const s of stringInputs) record(`new("${s}")`, 'new_string', () => new Fraction(s));
for (const [n, d] of pairInputs) record(`new(${n},${d})`, 'new_pair', () => new Fraction(n, d));
record('new(NaN)', 'new_nan', () => new Fraction(NaN));

for (const v of [0.5, -7/3, 2.54, 0]) {
  record(`abs(${v})`, 'abs', () => new Fraction(v).abs());
  record(`neg(${v})`, 'neg', () => new Fraction(v).neg());
  if (v !== 0) record(`inverse(${v})`, 'inverse', () => new Fraction(v).inverse());
  record(`clone(${v})`, 'clone', () => new Fraction(v).clone());
  record(`valueOf(${v})`, 'valueOf', () => new Fraction(v).valueOf());
  record(`toContinued(${v})`, 'toContinued', () => new Fraction(v).toContinued());
}

const binPairs = [[0.5, 0.25], [1/3, 1/3], [-7/3, 2], [127, 50], [2.54, 96]];
for (const [a, b] of binPairs) {
  record(`add(${a},${b})`, 'add', () => new Fraction(a).add(b));
  record(`sub(${a},${b})`, 'sub', () => new Fraction(a).sub(b));
  record(`mul(${a},${b})`, 'mul', () => new Fraction(a).mul(b));
  if (b !== 0) record(`div(${a},${b})`, 'div', () => new Fraction(a).div(b));
}

for (const v of dpcmInputs) {
  record(`dpcm(${v})`, 'dpcm', () => new Fraction(v).mul(2.54).div(96).simplify());
  record(`dpi(${v})`, 'dpi', () => new Fraction(v).div(96).simplify());
}

for (const eps of [undefined, 0.001, 0.1, 0, Infinity, -1]) {
  const label = `simplify(0.1, ${eps === undefined ? 'undefined' : eps})`;
  record(label, 'simplify', () => new Fraction(0.1).simplify(eps));
}

for (const [a, b] of [[0.5, 0.5], [1/3, 1/3], [0.5, 0.25], [0.25, 0.5]]) {
  record(`equals(${a},${b})`, 'equals', () => new Fraction(a).equals(b));
  record(`compare(${a},${b})`, 'compare', () => new Fraction(a).compare(b));
  record(`divisible(${a},${b})`, 'divisible', () => new Fraction(a).divisible(b));
}

for (const v of [0.123456, -0.123456, 1.5, -0.5]) {
  for (const places of [undefined, 0, 2, 4]) {
    const lab = (op) => `${op}(${v},${places === undefined ? 'undefined' : places})`;
    record(lab('ceil'), 'ceil', () => new Fraction(v).ceil(places));
    record(lab('floor'), 'floor', () => new Fraction(v).floor(places));
    record(lab('round'), 'round', () => new Fraction(v).round(places));
  }
}

for (const [base, exp] of [[2, 3], [2, -3], [4, 0.5], [-2, 3]]) {
  record(`pow(${base},${exp})`, 'pow', () => new Fraction(base).pow(exp));
}
record('pow(8,[1,3])', 'pow_pair', () => new Fraction(8).pow(1, 3));

for (const [a, b] of [[[5,8], [3,7]], [[6,1], [2,1]], [[13,3], [7,8]]]) {
  record(`gcd(${JSON.stringify(a)},${JSON.stringify(b)})`, 'gcd', () => new Fraction(a[0], a[1]).gcd(b[0], b[1]));
  record(`lcm(${JSON.stringify(a)},${JSON.stringify(b)})`, 'lcm', () => new Fraction(a[0], a[1]).lcm(b[0], b[1]));
  record(`mod(${JSON.stringify(a)},${JSON.stringify(b)})`, 'mod', () => new Fraction(a[0], a[1]).mod(b[0], b[1]));
}

for (const [a, b] of [[1,1], [7,3], [-7,3], [1,3], [22,7]]) {
  record(`toFraction(${a}/${b}, false)`, 'toFraction_false', () => new Fraction(a, b));
  record(`toFraction(${a}/${b}, true)`,  'toFraction_true',  () => new Fraction(a, b));
  record(`toLatex(${a}/${b}, false)`, 'toLatex_false', () => new Fraction(a, b));
  record(`toLatex(${a}/${b}, true)`,  'toLatex_true',  () => new Fraction(a, b));
}

fs.writeFileSync('crates/autoprefixer/tests/fraction_js/oracle.json', JSON.stringify(cases, null, 2));
console.log(`wrote ${cases.length} cases`);
