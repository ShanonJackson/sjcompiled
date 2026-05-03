import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const StyledDiv = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	StyledDiv.displayName = "StyledDiv";
}
