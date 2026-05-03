import React from "react";
import { gridSize } from "./jira-common-styles";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _3 = "._p12f1fra{max-width:75pc}";
const _2 = "._1ul91ogm{min-width:600px}";
const _ = "._1bsb7vkz{width:1pc}";
const Container = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1bsb7vkz _1ul91ogm _p12f1fra", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Container.displayName = "Container";
}
export const Component = () => <Container />;
