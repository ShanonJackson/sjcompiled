import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _7 = "._jf4cnqa1:hover{text-decoration-style:solid}";
const _6 = "._1bnx8stv:hover{text-decoration-line:underline}";
const _5 = "._9oik1r31:hover{text-decoration-color:currentColor}";
const _4 = "._syaz13q2{color:blue}";
const _3 = "._ajmmnqa1{text-decoration-style:solid}";
const _2 = "._1hmsglyw{text-decoration-line:none}";
const _ = "._4bfu1r31{text-decoration-color:currentColor}";
const StyledLink = forwardRef(({ as: C = "a", style: __cmpls, ...__cmplp }, __cmplr) => {
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
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_4bfu1r31 _1hmsglyw _ajmmnqa1 _syaz13q2 _9oik1r31 _1bnx8stv _jf4cnqa1", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	StyledLink.displayName = "StyledLink";
}
