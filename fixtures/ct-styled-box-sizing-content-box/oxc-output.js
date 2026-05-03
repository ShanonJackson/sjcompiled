import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._vchh1ntv{box-sizing:content-box}";
export const Component = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_vchh1ntv", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Component.displayName = "Component";
}
