import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _4 = "._15l21osq:after{top:100%}";
const _3 = "._18postnw:after{position:absolute}";
const _2 = "._aetr1oyl:after{content:\"tooltip\"}";
const _ = "._kqswh2mm{position:relative}";
const Tooltip = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_kqswh2mm _aetr1oyl _18postnw _15l21osq", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Tooltip.displayName = "Tooltip";
}
