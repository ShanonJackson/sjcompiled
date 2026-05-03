import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _9 = "._vwz4yu22{line-height:1.6}";
const _8 = "._1wybdlk8{font-size:14px}";
const _7 = "._syaz143u{color:navy}";
const _6 = "._1wyb1tcg{font-size:24px}";
const _5 = "._1e0c1txw{display:flex}";
const _4 = "._19bvgktf{padding-left:20px}";
const _3 = "._n3tdgktf{padding-bottom:20px}";
const _2 = "._u5f3gktf{padding-right:20px}";
const _ = "._ca0qgktf{padding-top:20px}";
const Container = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4,
		_5
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_ca0qgktf _u5f3gktf _n3tdgktf _19bvgktf _1e0c1txw", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Container.displayName = "Container";
}
const Title = forwardRef(({ as: C = "h1", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_6, _7]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1wyb1tcg _syaz143u", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Title.displayName = "Title";
}
const Paragraph = forwardRef(({ as: C = "p", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_8, _9]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1wybdlk8 _vwz4yu22", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Paragraph.displayName = "Paragraph";
}
