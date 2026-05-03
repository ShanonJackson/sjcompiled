import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _3 = "._oyoh2fjc:hover{--display-drag-handle:var(--_11m4mys)}";
const _2 = "._15y3r8wq:hover{--display-icon-before:var(--_b3nd5v)}";
const _ = "._tzy41kuy{opacity:.1}";
const tabStyles = null;
export const Component = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_tzy41kuy _15y3r8wq _oyoh2fjc", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Component.displayName = "Component";
}
