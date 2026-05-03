/** @jsxRuntime classic */
/** @jsx jsx */
import { jsx, cssMap, cx } from '@atlaskit/css';
import { Box } from '@atlaskit/primitives/compiled';
const styles = cssMap({
  card: {
    borderRadius: "var(--ds-radius-small, 4px)"
  },
  iconContainer: {
    borderRadius: "var(--ds-radius-small, 3px)",
    textAlign: 'center'
  }
});
export const IconContainer = () => <Box xcss={cx(styles.card)}>
		<Box as="span" xcss={cx(styles.iconContainer)}>
			icon
		</Box>
	</Box>;