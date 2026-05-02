import parser from '../../../node_modules/.bun/postcss-selector-parser@6.1.2/node_modules/postcss-selector-parser/dist/index.js';
import postcss from '../../../node_modules/.bun/postcss@8.5.6/node_modules/postcss/lib/postcss.mjs';
// Construct the plugin inline (mirror upstream src/index.js verbatim, but
// passing in our locally-imported parser so module resolution works).
const pseudoElements = new Set(['::before','::after','::first-letter','::first-line']);
const pseudoReplacements = new Map([
  [':nth-child', ':first-child'],
  [':nth-of-type', ':first-of-type'],
  [':nth-last-child', ':last-child'],
  [':nth-last-of-type', ':last-of-type'],
]);
const tagReplacements = new Map([['from','0%'],['100%','to']]);
const escapesRe = /\\([0-9A-Fa-f]{1,6})[ \t\n\f\r]?/g;
const rangeRe = /[\u0000-\u002c\u002e\u002f\u003A-\u0040\u005B-\u005E\u0060\u007B-\u009f]/;
function canUnquote(value) {
  if (value === '-' || value === '') return false;
  value = value.replace(escapesRe, 'a').replace(/\\./g, 'a');
  return !(rangeRe.test(value) || /^(?:-?\d|--)/.test(value));
}
function attribute(selector) {
  if (selector.value) {
    if (selector.raws.value) selector.raws.value = selector.raws.value.replace(/\\\n/g, '').trim();
    if (canUnquote(selector.value)) selector.quoteMark = null;
    if (selector.operator) selector.operator = selector.operator.trim();
  }
  selector.rawSpaceBefore = '';
  selector.rawSpaceAfter = '';
  selector.spaces.attribute = { before: '', after: '' };
  selector.spaces.operator = { before: '', after: '' };
  selector.spaces.value = { before: '', after: selector.insensitive ? ' ' : '' };
  if (selector.raws.spaces) {
    selector.raws.spaces.attribute = { before: '', after: '' };
    selector.raws.spaces.operator = { before: '', after: '' };
    selector.raws.spaces.value = { before: '', after: selector.insensitive ? ' ' : '' };
    if (selector.insensitive) selector.raws.spaces.insensitive = { before: '', after: '' };
  }
  selector.attribute = selector.attribute.trim();
}
function combinator(selector) {
  const value = selector.value.trim();
  selector.spaces.before = ''; selector.spaces.after = '';
  selector.rawSpaceBefore = ''; selector.rawSpaceAfter = '';
  selector.value = value.length ? value : ' ';
}
function pseudo(selector) {
  const value = selector.value.toLowerCase();
  if (selector.nodes.length === 1 && pseudoReplacements.has(value)) {
    const first = selector.at(0); const one = first.at(0);
    if (first.length === 1) {
      if (one.value === '1') selector.replaceWith(parser.pseudo({ value: pseudoReplacements.get(value) }));
      if (one.value && one.value.toLowerCase() === 'even') one.value = '2n';
    }
    if (first.length === 3) {
      const two = first.at(1); const three = first.at(2);
      if (one.value && one.value.toLowerCase() === '2n' && two.value === '+' && three.value === '1') {
        one.value = 'odd'; two.remove(); three.remove();
      }
    }
    return;
  }
  selector.walk((child) => {
    if (child.type === 'selector' && child.parent) {
      const uniques = new Set();
      child.parent.each((sibling) => {
        const s = String(sibling);
        if (!uniques.has(s)) uniques.add(s); else sibling.remove();
      });
    }
  });
  if (pseudoElements.has(value)) selector.value = selector.value.slice(1);
}
function tag(selector) {
  const value = selector.value.toLowerCase();
  if (tagReplacements.has(value)) selector.value = tagReplacements.get(value);
}
function universal(selector) {
  const next = selector.next();
  if (next && next.type !== 'combinator') selector.remove();
}
const reducers = new Map([['attribute', attribute],['combinator', combinator],['pseudo', pseudo],['tag', tag],['universal', universal]]);
function pluginCreator() {
  return {
    postcssPlugin: 'postcss-minify-selectors',
    OnceExit(css) {
      const cache = new Map();
      const processor = parser((selectors) => {
        const uniqueSelectors = new Set();
        selectors.walk((sel) => {
          sel.spaces.before = sel.spaces.after = '';
          const r = reducers.get(sel.type);
          if (r !== undefined) { r(sel); return; }
          const s = String(sel);
          if (sel.type === 'selector' && sel.parent && sel.parent.type !== 'pseudo') {
            if (!uniqueSelectors.has(s)) uniqueSelectors.add(s);
            else sel.remove();
          }
        });
        selectors.nodes.sort();
      });
      css.walkRules((rule) => {
        const selector = rule.raws.selector && rule.raws.selector.value === rule.selector
          ? rule.raws.selector.raw : rule.selector;
        if (selector[selector.length - 1] === ':') return;
        if (cache.has(selector)) { rule.selector = cache.get(selector); return; }
        const optimized = processor.processSync(selector);
        rule.selector = optimized;
        cache.set(selector, optimized);
      });
    },
  };
}
pluginCreator.postcss = true;
const minifySelectors = pluginCreator;

// Inspect parsed shape
const root = parser().astSync('.a, .a');
console.log('---AST shape---');
function dump(n, depth=0) {
  const ind = '  '.repeat(depth);
  console.log(`${ind}${n.type} value=${JSON.stringify(n.value)} spaces=${JSON.stringify(n.spaces)} rawSpaces={before:${JSON.stringify(n.rawSpaceBefore)}, after:${JSON.stringify(n.rawSpaceAfter)}}`);
  if (n.nodes) for (const c of n.nodes) dump(c, depth+1);
}
dump(root);

console.log('\n---String of each top-level Selector before any clear---');
root.nodes.forEach((s, i) => console.log(`[${i}] String=${JSON.stringify(String(s))}`));

console.log('\n---After clearing each Selector spaces---');
root.nodes.forEach((s) => { s.spaces.before = ''; s.spaces.after = ''; });
root.nodes.forEach((s, i) => console.log(`[${i}] String=${JSON.stringify(String(s))}`));

// Now run the actual plugin to verify final byte output.
console.log('\n---Plugin output for `.a, .a`---');
const out = postcss([minifySelectors()]).process('.a, .a { color: red; }', { from: undefined }).css;
console.log(JSON.stringify(out));

console.log('\n---Plugin output for `.a    .b`---');
const out2 = postcss([minifySelectors()]).process('.a    .b { color: red; }', { from: undefined }).css;
console.log(JSON.stringify(out2));

console.log('\n---Plugin output for `:is(.a, .b, .a)`---');
const out3 = postcss([minifySelectors()]).process(':is(.a, .b, .a) { color: red; }', { from: undefined }).css;
console.log(JSON.stringify(out3));

console.log('\n---Plugin output for `.b, .a`---');
const out4 = postcss([minifySelectors()]).process('.b, .a { color: red; }', { from: undefined }).css;
console.log(JSON.stringify(out4));
