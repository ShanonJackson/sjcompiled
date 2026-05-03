import { css } from '@compiled/react';
const styles = css({
  backgroundColor: 'color.background.discovery',
  borderRadius: "var(--ds-radius-large, 8px)",
  font: "var(--ds-font-body-UNSAFE_small, normal 400 12px/16px \"Atlassian Sans\", ui-sans-serif, -apple-system, BlinkMacSystemFont, \"Segoe UI\", Ubuntu, \"Helvetica Neue\", sans-serif)",
  color: "var(--ds-text-discovery, #803FA5)"
});
export const Component = () => <div css={styles} />;