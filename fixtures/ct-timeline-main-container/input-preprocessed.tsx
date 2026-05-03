/** @jsx jsx */
import React, { type FC } from 'react';
import { jsx } from '@compiled/react';
import { css } from '@atlaskit/css';
type Props = {
  children?: React.ReactNode;
};
export const MainContainer: FC<Props> = ({
  children,
  ...props
}) => {
  return <div css={styles.container} {...props}>
      {children}
    </div>;
};
const styles = css({
  display: 'flex',
  flexFlow: 'column nowrap',
  borderColor: "var(--ds-border, #0B120E24)",
  borderStyle: 'solid',
  borderRadius: "var(--ds-radius-large, 8px)",
  overflow: 'hidden',
  position: 'relative'
});