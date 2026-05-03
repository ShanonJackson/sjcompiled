import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _3 = "._1bsb1si1{width:var(--_u081c2)}";
const _2 = "._4t3i1si1{height:var(--_u081c2)}";
const _ = "._2rko1y44{border-radius:4px}";
const MAP = {
	small: 16,
	medium: 24
};
const Avatar = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmplp} style={{
		...__cmpls,
		"--_u081c2": ix(MAP[__cmplp.size], "px"),
		"--_u081c2": ix(MAP[__cmplp.size], "px")
	}} ref={__cmplr} className={ax(["_2rko1y44 _4t3i1si1 _1bsb1si1", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Avatar.displayName = "Avatar";
}
export default Avatar;
