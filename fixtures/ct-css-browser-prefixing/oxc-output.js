import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._1ul91ns9{min-width:-moz-fit-content;min-width:fit-content}";
const Container = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1ul91ns9", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Container.displayName = "Container";
}
export const Example = () => <Container>hello world</Container>;
