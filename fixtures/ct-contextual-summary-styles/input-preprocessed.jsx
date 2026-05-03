import { css, jsx } from '@compiled/react';
const media = {
  above: {
    xxs: '@media (min-width: 0rem)',
    sm: '@media (min-width: 48rem)'
  }
};
const styles = css({
  alignItems: 'center',
  display: 'flex',
  marginTop: "var(--ds-space-400, 32px)",
  paddingLeft: "var(--ds-space-100, 8px)",
  background: "var(--ds-surface, #FFFFFF)",
  borderRadius: "var(--ds-radius-small, 4px)",
  gap: "var(--ds-space-100, 8px)",
  [media.above.xxs]: {
    flexDirection: 'column'
  },
  [media.above.sm]: {
    flexDirection: 'row'
  },
  span: {
    color: `${"var(--ds-text, #292A2E)"} !important`
  }
});
export const Component = () => <div css={styles}>
    <span>Hi</span>
  </div>;