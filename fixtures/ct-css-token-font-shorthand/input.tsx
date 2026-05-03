import { css } from '@compiled/react';
import { token } from '@atlaskit/tokens';

const styles = css({
  backgroundColor: 'color.background.discovery',
  borderRadius: token('radius.large'),
  font: token('font.body.small'),
  color: token('color.text.discovery'),
});

export const Component = () => <div css={styles} />;
