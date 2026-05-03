import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _3 = "@keyframes kgnpaw5{0%{opacity:0}to{opacity:1}}";
const _2 = "._5sag9cwz{animation-duration:1s}";
const _ = "._j7hq1wwu{animation-name:kgnpaw5}";
const fadeIn = null;
const FadeInDiv = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_j7hq1wwu _5sag9cwz", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	FadeInDiv.displayName = "FadeInDiv";
}
