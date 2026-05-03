import React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._syazsxvs{color:var(--_1xxt5am)}";
export const Component = (props) => {
	// eslint-disable-next-line @atlaskit/ui-styling-standard/no-styled -- parity with production usage
	const Label = forwardRef(({ as: C = "h5", style: __cmpls, ...__cmplp }, __cmplr) => {
		if (__cmplp.innerRef) {
			throw new Error("Please use 'ref' instead of 'innerRef'.");
		}
		return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={{
			...__cmpls,
			"--_1xxt5am": ix(props.isDisabled ? 'var(--ds-text-disabled, #080F214A)' : 'var(--ds-text, #292A2E)')
		}} ref={__cmplr} className={ax(["_syazsxvs", __cmplp.className])} />
      </CC>;
	});
	return <Label>text</Label>;
};
if (process.env.NODE_ENV !== "production") {
	Label.displayName = "Label";
}
