import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _6 = "._bfhk13q2{background-color:blue}";
const _5 = "._syazu67f{color:#fff}";
const _4 = "._19bvgktf{padding-left:20px}";
const _3 = "._n3td19bv{padding-bottom:10px}";
const _2 = "._u5f3gktf{padding-right:20px}";
const _ = "._ca0q19bv{padding-top:10px}";
const StyledButton = forwardRef(({ as: C = "button", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4,
		_5,
		_6
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_ca0q19bv _u5f3gktf _n3td19bv _19bvgktf _syazu67f _bfhk13q2", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	StyledButton.displayName = "StyledButton";
}
