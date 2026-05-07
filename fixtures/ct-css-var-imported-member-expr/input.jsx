// Minimal repro from the AFM monorepo team.
//
// Trigger: a CSS custom property (`--ds-text`) whose value is a
// member expression on an IMPORTED constant (`Tokens.COLOR_TEXT`,
// where `Tokens` is a namespace/object imported from a sibling
// module).
//
// Babel resolves the import via the configured resolver, inlines
// the imported `Tokens` object, and reads `Tokens.COLOR_TEXT` —
// emitting the resolved string value into the generated CSS.
//
// SWC port reportedly panics with "must be statically defined"
// because the static-evaluation path doesn't follow imported
// member expressions through to the resolved binding.
//
// Distilled from a downstream report:
//   "It's the '--ds-text': Tokens.COLOR_TEXT line — a CSS custom
//    property whose value is a member-expression (Tokens.COLOR_TEXT,
//    an imported constant) rather than a string literal."
import React from 'react';
import { styled } from '@compiled/react';
import { Tokens } from './tokens';

const Box = styled.div({
  color: 'red',
  '--ds-text': Tokens.COLOR_TEXT,
});

export default function App() {
  return <Box>hello</Box>;
}
