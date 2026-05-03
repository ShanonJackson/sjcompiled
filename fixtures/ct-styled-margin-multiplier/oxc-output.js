import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _3 = "._1e0c1txw{display:flex}";
const _2 = "._2hwx1meq{margin-right:var(--_oytggz)}";
const _ = "._18u0t6e2{margin-left:var(--_150w5uv)}";
const marginMultiplier = (hasScrolled, isShadowVisible) => {
	if (isShadowVisible) return 0;
	if (hasScrolled) return -1;
	return 1;
};
const getMargin = (hasScrolled, isShadowVisible) => 8 * marginMultiplier(hasScrolled, isShadowVisible);
const Wrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { hasScrolled, leftShadowVisible, rightShadowVisible, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_150w5uv": ix(`${getMargin(__cmplp.hasScrolled, __cmplp.leftShadowVisible)}px`),
		"--_oytggz": ix(`${getMargin(__cmplp.hasScrolled, __cmplp.rightShadowVisible)}px`)
	}} ref={__cmplr} className={ax(["_18u0t6e2 _2hwx1meq _1e0c1txw", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Wrapper.displayName = "Wrapper";
}
export const Component = (props) => <Wrapper {...props}>Content</Wrapper>;
