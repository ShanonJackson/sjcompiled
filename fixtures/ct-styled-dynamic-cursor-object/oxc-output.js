import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._wtsb13gf cursor{cursor:not-allowed}";
const Button = forwardRef(({ as: C = "button", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_wtsb13gf", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Button.displayName = "Button";
}
export const Component = ({ isDisabled, isFromServerSide }) => <Button isDisabled={isDisabled} isFromServerSide={isFromServerSide}>
		Assignee
	</Button>;
