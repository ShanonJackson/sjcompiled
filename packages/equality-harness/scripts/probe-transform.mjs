import { transformCss } from '@compiled/css';

const cases = [
  '._x{font-size: 16px}',
  '._x{font-size: 0.5rem}',
  '._x:after{content:""}',
  "._x:after{content: ''}",
];

for (const css of cases) {
  delete process.env.COMPILED_CSS_ENGINE;
  const js = transformCss(css, {});
  process.env.COMPILED_CSS_ENGINE = 'rust';
  const rust = transformCss(css, {});
  console.log(`INPUT: ${css}`);
  console.log(`  JS:   ${JSON.stringify(js.sheets)}`);
  console.log(`  RUST: ${JSON.stringify(rust.sheets)}`);
  console.log(`  EQ:   ${JSON.stringify(js.sheets) === JSON.stringify(rust.sheets)}`);
  console.log('');
}
