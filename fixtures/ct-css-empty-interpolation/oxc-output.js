import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _10 = "._bfhk32ev{background-color:pink}";
const _9 = "._ajmmnqa1{text-decoration-style:solid}";
const _8 = "._1hmsglyw{text-decoration-line:none}";
const _7 = "._4bfu1r31{text-decoration-color:currentColor}";
const _6 = "._4t3i19bv{height:10px}";
const _5 = "._kqswh2mm{position:relative}";
const _4 = "._19bvi2wt{padding-left:6px}";
const _3 = "._n3tdidpf{padding-bottom:0}";
const _2 = "._u5f3i2wt{padding-right:6px}";
const _ = "._ca0qidpf{padding-top:0}";
const Component = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { flag, other, ...__cmpldp } = __cmplp;
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
		_9,
		_10
	]}</CS>
        <C {...__cmpldp} style={__cmpls} ref={__cmplr} className={ax([
		"_ca0qidpf _u5f3i2wt _n3tdidpf _19bvi2wt _kqswh2mm",
		__cmplp.flag && "_4t3i19bv",
		__cmplp.other && "_4bfu1r31 _1hmsglyw _ajmmnqa1 _bfhk32ev",
		__cmplp.className
	])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Component.displayName = "Component";
}
export default Component;
