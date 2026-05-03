import React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._c71ly3ze{max-height:var(--_2gckvy)}";
const ClampedHighlight = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { lineclamp, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_2gckvy": ix(`${__cmplp.lineclamp * 1.42857142857143}em`)
	}} ref={__cmplr} className={ax(["_c71ly3ze", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	ClampedHighlight.displayName = "ClampedHighlight";
}
export const Example = (props) => <ClampedHighlight {...props}>example</ClampedHighlight>;
