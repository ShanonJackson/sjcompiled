import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _2 = "._k48p8n31{font-weight:bold}";
const _ = "._syaz13q2{color:blue}";
const StyledDiv = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_syaz13q2", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	StyledDiv.displayName = "StyledDiv";
}
const MyComponent = () => <CC>
  <CS>{[_2]}</CS>
  {<span className={ax(["_k48p8n31"])}>Bold</span>}
  </CC>;
