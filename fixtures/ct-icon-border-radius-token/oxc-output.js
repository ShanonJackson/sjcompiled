import { cx } from "@atlaskit/css";
import { Box } from "@atlaskit/primitives/compiled";
import _React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _3 = "._y3gn1h6o{text-align:center}";
const _2 = "._2rkofajl{border-radius:var(--ds-radius-small,3px)}";
const _ = "._2rko12b0{border-radius:var(--ds-radius-small,4px)}";
const styles = {
	card: "_2rko12b0",
	iconContainer: "_2rkofajl _y3gn1h6o"
};
export const IconContainer = () => <CC>
  <CS>{[
	_,
	_2,
	_3
]}</CS>
  {<Box xcss={cx(styles.card)}>
		<CC>
  <CS>{[
	_,
	_2,
	_3
]}</CS>
  {<Box as="span" xcss={cx(styles.iconContainer)}>
			icon
		</Box>}
  </CC>
	</Box>}
  </CC>;
