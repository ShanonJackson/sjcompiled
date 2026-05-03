import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _2 = "._1wybdlk8{font-size:14px}";
const _ = "._syaz1cj8{color:var(--_xexnhp)}";
const DynamicDiv = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_, _2]}</CS>
        <C {...__cmplp} style={{
		...__cmpls,
		"--_xexnhp": ix(__cmplp.color)
	}} ref={__cmplr} className={ax(["_syaz1cj8 _1wybdlk8", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	DynamicDiv.displayName = "DynamicDiv";
}
