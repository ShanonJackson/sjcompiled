import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _7 = "._syazr3uz{color:#000}";
const _6 = "._bfhku67f{background-color:#fff}";
const _5 = "._2rkoftgi{border-radius:8px}";
const _4 = "._19bv7vkz{padding-left:1pc}";
const _3 = "._n3td7vkz{padding-bottom:1pc}";
const _2 = "._u5f37vkz{padding-right:1pc}";
const _ = "._ca0q7vkz{padding-top:1pc}";
const bgColor = "white";
const textColor = "black";
const Card = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
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
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_ca0q7vkz _u5f37vkz _n3td7vkz _19bv7vkz _2rkoftgi _bfhku67f _syazr3uz", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Card.displayName = "Card";
}
