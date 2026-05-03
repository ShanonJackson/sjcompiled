import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _2 = "@media (max-width:768px){._1ecodlk8{font-size:14px}}";
const _ = "._1wyb7vkz{font-size:1pc}";
const ResponsiveDiv = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_, _2]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1wyb7vkz _1ecodlk8", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	ResponsiveDiv.displayName = "ResponsiveDiv";
}
