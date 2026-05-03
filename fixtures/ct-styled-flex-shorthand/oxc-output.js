import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _3 = "._i0dl1wug{flex-basis:auto}";
const _2 = "._1o9zidpf{flex-shrink:0}";
const _ = "._16jlidpf{flex-grow:0}";
const Cell = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_16jlidpf _1o9zidpf _i0dl1wug", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Cell.displayName = "Cell";
}
export const Component = () => <Cell>Content</Cell>;
