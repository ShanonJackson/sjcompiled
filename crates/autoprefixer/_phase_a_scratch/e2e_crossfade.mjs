import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const AFM = ['last 2 Edge version','last 2 Firefox version','last 5 Chrome version','last 2 Safari version','last 2 iOS version','last 2 ChromeAndroid version'];
const autoprefixer = require('autoprefixer');
const postcss = require('postcss');
const css = `.x { background-image: cross-fade(url('a.png'), url('b.png'), 50%); }\n`;
const r = await postcss([autoprefixer({ overrideBrowserslist: AFM })]).process(css, { from: 'a.css', to: 'a.css' });
console.log(JSON.stringify(r.css));
