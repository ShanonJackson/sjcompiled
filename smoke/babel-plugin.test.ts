import { describe, expect, test } from 'bun:test';
import { transformSync } from '@babel/core';
import compiledPlugin from '@compiled/babel-plugin';

const transform = (code: string, filename = 'test.tsx') =>
  transformSync(code, {
    filename,
    babelrc: false,
    configFile: false,
    plugins: [compiledPlugin],
    presets: [],
  })?.code ?? '';

describe('@compiled/babel-plugin', () => {
  test('transforms styled.h1 template literal into atomic CSS + runtime', () => {
    const out = transform(`
      import { styled } from '@compiled/react';
      const Big = styled.h1\`
        font-size: 48px;
        color: rebeccapurple;
      \`;
    `);

    expect(out).toContain('@compiled/react/runtime');
    expect(out).toContain('CC');
    expect(out).toContain('CS');
    expect(out).toMatch(/font-size:\s*3pc|font-size:\s*48px/);
    expect(out).toContain('color:#639');
  });

  test('transforms css prop usage into atomic class assignment', () => {
    const out = transform(`
      /** @jsxImportSource @compiled/react */
      import { css } from '@compiled/react';
      const styles = css({ color: 'red', fontSize: '12px' });
      const Foo = () => <div css={styles}>hi</div>;
    `);

    expect(out).toContain('color:red');
    expect(out).toMatch(/font-size:\s*(12px|9pt)/);
    expect(out).toContain('@compiled/react/runtime');
  });

  test('transforms keyframes into atomic CSS', () => {
    const out = transform(`
      import { keyframes, styled } from '@compiled/react';
      const fadeIn = keyframes\`
        from { opacity: 0; }
        to   { opacity: 1; }
      \`;
      const Spinner = styled.div\`
        animation: \${fadeIn} 1s;
      \`;
    `);

    expect(out).toContain('@keyframes');
    expect(out).toContain('opacity:0');
    expect(out).toContain('opacity:1');
  });

  test('emits a deterministic hash for the same input', () => {
    const code = `
      import { styled } from '@compiled/react';
      const A = styled.span\`color: red;\`;
    `;
    expect(transform(code)).toBe(transform(code));
  });

  test('passes through non-compiled code unchanged (no CC/CS injected)', () => {
    const out = transform(`
      const x = 1;
      export default x;
    `);
    expect(out).not.toContain('CC');
    expect(out).not.toContain('CS');
    expect(out).toContain('const x = 1');
  });
});
