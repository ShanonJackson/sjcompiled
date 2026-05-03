import React from "react";
import { layers } from "./source";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _2 = "._kqsw1n9t{position:fixed}";
const _ = "._1pby1r1z{z-index:65}";
const Wrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_, _2]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1pby1r1z _kqsw1n9t", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Wrapper.displayName = "Wrapper";
}
export const Component = () => <Wrapper />;
