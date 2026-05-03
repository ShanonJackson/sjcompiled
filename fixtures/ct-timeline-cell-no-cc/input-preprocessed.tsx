import React, { useCallback, useState } from 'react';
import { cssMap } from '@atlaskit/css';
import { Box } from '@atlaskit/primitives/compiled';
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
export default function Component({
  item
}: {
  item: any;
}) {
  const [hover, setHover] = useState(false);
  const onEnter = useCallback(() => setHover(true), []);
  const onLeave = useCallback(() => setHover(false), []);
  const onClick = useCallback(() => console.log(item?.key), [item?.key]);
  return <Box xcss={styles.cellWrapper} onMouseEnter={onEnter} onMouseLeave={onLeave}>
      <div onClick={onClick}>{hover ? 'Hover' : 'Idle'}</div>
    </Box>;
}