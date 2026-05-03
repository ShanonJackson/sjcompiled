import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _7 = "._yv0e16hb{grid-template-columns:repeat(3,1fr)}";
const _6 = "._1e0c11p5{display:grid}";
const _5 = "._19bvgktf{padding-left:20px}";
const _4 = "._n3tdgktf{padding-bottom:20px}";
const _3 = "._u5f3gktf{padding-right:20px}";
const _2 = "._ca0qgktf{padding-top:20px}";
const _ = "._zulp7vkz{gap:1pc}";
const Grid = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
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
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_zulp7vkz _ca0qgktf _u5f3gktf _n3tdgktf _19bvgktf _1e0c11p5 _yv0e16hb", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Grid.displayName = "Grid";
}
