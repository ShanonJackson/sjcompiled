import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._1itkh4uj{background-image:linear-gradient(to right,var(--ds-background-neutral,#0515240f) 10%,var(--ds-background-neutral-subtle,#00000000) 30%,var(--ds-background-neutral,#0515240f) 50%)}";
const Box = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1itkh4uj", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Box.displayName = "Box";
}
export const Component = () => <Box />;
