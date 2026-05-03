import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _9 = "._p12f1fra{max-width:75pc}";
const _8 = "._18u01wug{margin-left:auto}";
const _7 = "._otyridpf{margin-bottom:0}";
const _6 = "._2hwx1wug{margin-right:auto}";
const _5 = "._19pkidpf{margin-top:0}";
const _4 = "._19bvgktf{padding-left:20px}";
const _3 = "._n3td1ylp{padding-bottom:40px}";
const _2 = "._u5f3gktf{padding-right:20px}";
const _ = "._ca0q1ylp{padding-top:40px}";
const Section = forwardRef(({ as: C = "section", style: __cmpls, ...__cmplp }, __cmplr) => {
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
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_ca0q1ylp _u5f3gktf _n3td1ylp _19bvgktf _19pkidpf _2hwx1wug _otyridpf _18u01wug _p12f1fra", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Section.displayName = "Section";
}
