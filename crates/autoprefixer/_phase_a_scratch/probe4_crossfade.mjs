import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const AFM = ['last 2 Edge version','last 2 Firefox version','last 5 Chrome version','last 2 Safari version','last 2 iOS version','last 2 ChromeAndroid version'];

// Patch Value.save and CrossFade prototype before requiring autoprefixer
const Value = require('autoprefixer/lib/value');
const origSave = Value.save;
Value.save = function(prefixes, decl) {
  console.log('Value.save called for prop=', decl.prop, 'value=', JSON.stringify(decl.value), '_autoprefixerValues=', JSON.stringify(decl._autoprefixerValues));
  return origSave.call(this, prefixes, decl);
};

const autoprefixer = require('autoprefixer');
const postcss = require('postcss');
const css = `.x { background-image: cross-fade(url('a.png'), url('b.png'), 50%); }\n`;
const r = await postcss([autoprefixer({ overrideBrowserslist: AFM })]).process(css, { from: 'a.css', to: 'a.css' });
console.log('---FINAL---');
console.log(JSON.stringify(r.css));
