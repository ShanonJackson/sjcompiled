// Sanity test: does `width: fit-content` get prefixed for AFM?
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const AFM = ['last 2 Edge version', 'last 2 Firefox version', 'last 5 Chrome version', 'last 2 Safari version', 'last 2 iOS version', 'last 2 ChromeAndroid version'];
process.env.BROWSERSLIST = AFM.join(',');
const autoprefixer = require('autoprefixer');
const postcss = require('postcss');
const css = `
.fit { width: fit-content; }
.fill { width: fill-available; }
.stretch { width: stretch; }
.user { user-select: none; }
.text { text-decoration: underline; }
.cross { background: cross-fade(url(a.png), url(b.png), 50%); }
.element { background: -moz-element(#foo); }
.transition { transition: transform 0.2s ease; }
@supports (display: grid) { .a { display: grid; } }
`;
const result = await postcss([autoprefixer({ overrideBrowserslist: AFM })]).process(css, { from: 'a.css', to: 'a.css' });
console.log(result.css);
