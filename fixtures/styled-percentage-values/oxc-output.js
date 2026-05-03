import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _3 = "._1tkeuuw1{min-height:200px}";
const _2 = "._4t3idtre{height:50vh}";
const _ = "._1bsb1osq{width:100%}";
const FullWidth = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1bsb1osq _4t3idtre _1tkeuuw1", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	FullWidth.displayName = "FullWidth";
}
