import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _5 = "._syazijt0{color:var(--_1qrlcdp)}";
const _4 = "._19bv19bv{padding-left:10px}";
const _3 = "._n3td19bv{padding-bottom:10px}";
const _2 = "._u5f319bv{padding-right:10px}";
const _ = "._ca0q19bv{padding-top:10px}";
const ColorDiv = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { textColor, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4,
		_5
	]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_1qrlcdp": ix(__cmplp.textColor)
	}} ref={__cmplr} className={ax(["_ca0q19bv _u5f319bv _n3td19bv _19bv19bv _syazijt0", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	ColorDiv.displayName = "ColorDiv";
}
