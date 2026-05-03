import React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _4 = "._ajcj1wq8 >input::placeholder{font-weight:var(--ds-font-weight-medium,500)}";
const _3 = "._ajcj1wq8 >input::-moz-placeholder{font-weight:var(--ds-font-weight-medium,500)}";
const _2 = "._ppeni7uo >input::placeholder{color:var(--ds-text,#292a2e)}";
const _ = "._ppeni7uo >input::-moz-placeholder{color:var(--ds-text,#292a2e)}";
const Wrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
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
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_ppeni7uo _ajcj1wq8", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Wrapper.displayName = "Wrapper";
}
export const Component = () => <Wrapper>
		<input placeholder="Example" />
	</Wrapper>;
