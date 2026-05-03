import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _3 = "._1e0c1ule{display:block}";
const _2 = "._1wybdlk8{font-size:14px}";
const _ = "._syaz13q2{color:blue}";
const fontSize = 14;
const StyledDiv = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_syaz13q2 _1wybdlk8 _1e0c1ule", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	StyledDiv.displayName = "StyledDiv";
}
