import { css } from '@compiled/react';
import { xcss } from '@atlaskit/primitives';
const legacyStyles = xcss({
  display: 'inline-flex',
  columnGap: 'space.050',
  maxWidth: '100%',
  alignItems: 'baseline',
  justifyContent: 'center'
});
const refreshedStyles = css({
  width: 'auto',
  maxWidth: '100%',
  alignItems: 'baseline',
  justifyContent: 'center',
  columnGap: "var(--ds-space-050, 4px)",
  borderRadius: "var(--ds-radius-small, 4px)",
  borderWidth: "var(--ds-border-width, 1px)",
  borderStyle: 'solid',
  borderColor: 'transparent',
  maxHeight: '32px',
  font: "var(--ds-font-body, normal 400 14px/20px \"Atlassian Sans\", ui-sans-serif, -apple-system, BlinkMacSystemFont, \"Segoe UI\", Ubuntu, \"Helvetica Neue\", sans-serif)",
  fontWeight: "var(--ds-font-weight-medium, 500)",
  paddingBlock: "var(--ds-space-075, 6px)",
  paddingInline: "var(--ds-space-150, 12px)",
  cursor: 'pointer',
  '&:focus, &:active, &:focus-visible': {
    outline: `${"var(--ds-border-width-focused, 2px)"} auto ${"var(--ds-border-focused, #4688EC)"}`,
    outlineOffset: "var(--ds-space-negative-025, -2px)"
  }
});
export const Example = () => <>
    <div xcss={legacyStyles}>legacy</div>
    <div css={refreshedStyles}>refresh</div>
  </>;