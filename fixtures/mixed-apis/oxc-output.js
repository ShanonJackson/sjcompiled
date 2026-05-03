import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _8 = "._syazbf54{color:green}";
const _7 = "._syaz13q2{color:blue}";
const _6 = "._19bv19bv{padding-left:10px}";
const _5 = "._n3td19bv{padding-bottom:10px}";
const _4 = "._u5f319bv{padding-right:10px}";
const _3 = "._ca0q19bv{padding-top:10px}";
const _2 = "._k48p8n31{font-weight:bold}";
const _ = "._1wyb1tcg{font-size:24px}";
const StyledHeader = forwardRef(({ as: C = "h1", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_, _2]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1wyb1tcg _k48p8n31", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	StyledHeader.displayName = "StyledHeader";
}
const MyComponent = () => <div>
    <StyledHeader>Title</StyledHeader>
    <CC>
  <CS>{[
	_3,
	_4,
	_5,
	_6,
	_7
]}</CS>
  {<div className={ax(["_ca0q19bv _u5f319bv _n3td19bv _19bv19bv _syaz13q2"])}>Content</div>}
  </CC>
    <CC>
  <CS>{[_8]}</CS>
  {<span className={ax(["_syazbf54"])}>Status</span>}
  </CC>
  </div>;
