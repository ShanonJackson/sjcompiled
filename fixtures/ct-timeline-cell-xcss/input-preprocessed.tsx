import React from 'react';
import { cssMap } from '@atlaskit/css';
import { Box } from '@atlaskit/primitives/compiled';
export default function Component() {
  return <Box xcss={styles.cellWrapper}>Timeline cell</Box>;
}
const styles = cssMap({
  cellWrapper: {
    height: '100%',
    inset: "var(--ds-space-0, 0px)",
    paddingRight: "var(--ds-space-100, 8px)",
    paddingLeft: "var(--ds-space-100, 8px)",
    position: 'relative',
    width: '100%',
    alignItems: 'center',
    display: 'flex',
    overflowX: 'hidden'
  }
});