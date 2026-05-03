import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._18s81orz{margin:var(--_1kkgk2k)}";
const Styled = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={{
		...__cmpls,
		"--_1kkgk2k": ix(`${`${__cmplp.size}px`}`)
	}} ref={__cmplr} className={ax(["_18s81orz", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Styled.displayName = "Styled";
}
export const Component = () => <Styled size={10} />;
