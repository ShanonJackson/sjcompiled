import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _2 = "@keyframes k1poetz8{0%{transform:scale(1)}50%{transform:scale(1.1)}to{transform:scale(1)}}";
const _ = "._y44v1mmd{animation:k1poetz8 2s infinite}";
const pulse = null;
const StyledDiv = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_, _2]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_y44v1mmd", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	StyledDiv.displayName = "StyledDiv";
}
export const Component = () => <StyledDiv>Pulse</StyledDiv>;
