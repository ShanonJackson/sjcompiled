import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._11sqftgi p:nth-of-type(n+2){margin-top:8px}";
// Nested pseudo selector using "n+2" form that currently hashes differently between Babel and SWC.
export const Body = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_11sqftgi", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Body.displayName = "Body";
}
