import { describe, expect, test } from 'bun:test';
import { transformSync } from '@babel/core';
import jsxSyntax from '@babel/plugin-syntax-jsx';
import compiledPlugin from '@sjcompiled/babel-plugin';
import stripRuntimePlugin from '@sjcompiled/babel-plugin-strip-runtime';

type StyleRules = string[];
interface Metadata { styleRules?: StyleRules }

const stripTransform = (code: string, filename = 'test.tsx') => {
  const compiled = transformSync(code, {
    filename,
    babelrc: false,
    configFile: false,
    plugins: [compiledPlugin],
    presets: [],
  })?.code;

  if (compiled == null) throw new Error('compiled stage produced no code');

  const result = transformSync(compiled, {
    filename,
    babelrc: false,
    configFile: false,
    plugins: [
      jsxSyntax,
      [stripRuntimePlugin, { compiledRequireExclude: true }],
    ],
    presets: [],
  });

  return {
    code: result?.code ?? '',
    metadata: (result?.metadata ?? {}) as Metadata,
  };
};

describe('@sjcompiled/babel-plugin-strip-runtime', () => {
  test('extracts atomic style rules into metadata and strips CC/CS imports', () => {
    const { code, metadata } = stripTransform(`
      import { styled } from '@compiled/react';
      const Big = styled.h1\`
        font-size: 48px;
        color: rebeccapurple;
      \`;
    `);

    expect(metadata.styleRules).toBeDefined();
    expect(metadata.styleRules!.length).toBeGreaterThan(0);
    const flat = metadata.styleRules!.join(' ');
    expect(flat).toContain('color:#639');
    expect(flat).toMatch(/font-size:/);

    expect(code).not.toMatch(/\bCC\b/);
    expect(code).not.toMatch(/\bCS\b/);
  });

  test('preserves the underlying JSX element after stripping', () => {
    const { code } = stripTransform(`
      import { styled } from '@compiled/react';
      const Big = styled.h1\`color: red;\`;
    `);

    expect(code).toContain('h1');
  });
});
