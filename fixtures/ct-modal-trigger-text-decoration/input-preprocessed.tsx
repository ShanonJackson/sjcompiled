import React from 'react';
import { cssMap } from '@atlaskit/css';
import { Pressable, Text } from '@atlaskit/primitives/compiled';
const styles = cssMap({
  trigger: {
    backgroundColor: 'transparent',
    width: 'fit-content',
    paddingTop: "var(--ds-space-0, 0px)",
    paddingRight: "var(--ds-space-0, 0px)",
    paddingBottom: "var(--ds-space-0, 0px)",
    paddingLeft: "var(--ds-space-0, 0px)",
    textDecoration: 'underline',
    borderRadius: "var(--ds-radius-small, 4px)",
    '&:hover': {
      textDecoration: 'auto'
    }
  }
});
export const Example = () => <Pressable xcss={styles.trigger}>
		<Text size="large" weight="bold" color="color.text">
			Trigger content
		</Text>
	</Pressable>;