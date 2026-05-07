// Minimal repro: addComponentName=true on a `styled.X` definition.
//
// Babel emits the component-name class `c_Icon` as the first entry in
// the `ax([...])` call (alongside the atomic class hashes); the SWC
// port silently drops it.
//
// Distilled from ~593 jira files (cluster #1 of the AFM-jira parity run);
// this same shape covers ~2000 files when combined with clusters #2–#4
// (different component names, identical bug).
//
// Run with `addComponentName: true` in plugin opts (see ./opts.json).
//
// Original source pattern:
//   jira/src/packages/aais/timeline-legend/src/ui/symbols/color-by-icon/index.tsx
import React from 'react';
import { styled } from '@compiled/react';

type Props = { backgroundColor: string };

const Icon = styled.span<Props>({
  width: '8px',
  height: '8px',
  borderRadius: '4px',
  backgroundColor: ({ backgroundColor }) => backgroundColor,
});

const ColourByIcon = ({ backgroundColor }: Props): React.JSX.Element => (
  <Icon backgroundColor={backgroundColor} />
);

export default ColourByIcon;
