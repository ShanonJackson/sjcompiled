import postcss_old from 'postcss';
import postcss_new from '../node_modules/postcss-8-5-6/lib/postcss.js';

const inputs = [
  'a { color: red; }',
  'a { color: red }',
  'a {\n  color: red;\n  font-size: 12px;\n}',
  '@media (max-width: 100px) {\n  a { color: red; }\n}',
  'a { color: /* hi */ red; }',
  '@charset "utf-8";',
  'a {}',
  'a { color: red !important; }',
  'a { background: url(foo.png); }',
  'a { _color: red; }',
  '/* comment */ a { color: red; }',
  'a, b, c { color: red; }',
  ':root { --x: 5; }',
  'a { color: red; } b { color: blue; }',
  '@supports (display: grid) { .a { display: grid; } }',
  '.foo, .bar,\n.baz {\n  color: red;\n}',
  'a {\r\n  color: red;\r\n}',
  '/*! important comment */ a { color: red; }',
  '.a { background: linear-gradient(to right, red, blue); }',
  '@keyframes spin { 0% { transform: rotate(0deg); } 100% { transform: rotate(360deg); } }',
];
let mismatches = 0;
for (const css of inputs) {
  const a = postcss_old.parse(css).toString();
  const b = postcss_new.parse(css).toString();
  if (a !== b) {
    mismatches++;
    console.log('DIVERGENCE:', JSON.stringify(css));
    console.log('  8.4.31:', JSON.stringify(a));
    console.log('  8.5.6: ', JSON.stringify(b));
  }
}
console.log('Total inputs:', inputs.length, 'Mismatches:', mismatches);
