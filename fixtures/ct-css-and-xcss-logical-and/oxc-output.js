import { Inline, xcss } from "@atlaskit/primitives";
import _React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _ = "._1ul9ylrw{min-width:var(--space-0,0)}";
const ListWithPopup = ({ items, ItemComponent, maxLimit, initialIsOpen, isHoverPopoverEnabled }) => {
	return <Inline space="space.100" alignBlock="center" shouldWrap={!isHoverPopoverEnabled} xcss={isHoverPopoverEnabled && hoverPopoverStyles}>
      <CC>
  <CS>{[_]}</CS>
  {<ItemComponent isHoverPopoverEnabled={isHoverPopoverEnabled} className={ax([isHoverPopoverEnabled && "_1ul9ylrw"])} />}
  </CC>
    </Inline>;
};
const hoverPopoverStyles = xcss({
	paddingRight: "var(--space-50, 50px)",
	width: "100%"
});
const hoverPopoverItemStyles = null;
export default ListWithPopup;
