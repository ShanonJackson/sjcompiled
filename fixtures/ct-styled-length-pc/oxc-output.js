import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._1bsb4tsx{width:60pc}";
const Box = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1bsb4tsx", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Box.displayName = "Box";
}
export const Component = () => <Box>content</Box>;
