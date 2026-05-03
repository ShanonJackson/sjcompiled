import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _9 = "._80omtlke{cursor:pointer}";
const _8 = "._syazu67f{color:#fff}";
const _7 = "._bfhk13q2{background-color:blue}";
const _6 = "._19itglyw{border:none}";
const _5 = "._2rko1y44{border-radius:4px}";
const _4 = "._19bv7vkz{padding-left:1pc}";
const _3 = "._n3tdftgi{padding-bottom:8px}";
const _2 = "._u5f37vkz{padding-right:1pc}";
const _ = "._ca0qftgi{padding-top:8px}";
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
		_6,
		_7,
		_8,
		_9
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_ca0qftgi _u5f37vkz _n3tdftgi _19bv7vkz _2rko1y44 _19itglyw _bfhk13q2 _syazu67f _80omtlke", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	StyledButton.displayName = "StyledButton";
}
