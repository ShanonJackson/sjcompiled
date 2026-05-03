import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._syaz1q9v{color:hotpink}";
const Base = ({ children }) => <button>{children}</button>;
export const StyledButton = forwardRef(({ as: C = Base, style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_syaz1q9v", __cmplp.className])} />
      </CC>;
});
export const Component = () => <StyledButton>Click me</StyledButton>;
if (process.env.NODE_ENV !== "production") {
	StyledButton.displayName = "StyledButton";
}
