import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._syaz5scu{color:red}";
const RedText = forwardRef(({ as: C = "span", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_syaz5scu", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	RedText.displayName = "RedText";
}
