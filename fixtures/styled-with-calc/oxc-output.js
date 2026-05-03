import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _7 = "._18u0r0r7{margin-left:250px}";
const _6 = "._1bsb16eo{width:calc(100% - 250px)}";
const _5 = "._1e0c1txw{display:flex}";
const _4 = "._19bvgktf{padding-left:20px}";
const _3 = "._n3tdgktf{padding-bottom:20px}";
const _2 = "._u5f3gktf{padding-right:20px}";
const _ = "._ca0qgktf{padding-top:20px}";
const SidebarLayout = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
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
		_7
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_ca0qgktf _u5f3gktf _n3tdgktf _19bvgktf _1e0c1txw _1bsb16eo _18u0r0r7", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	SidebarLayout.displayName = "SidebarLayout";
}
