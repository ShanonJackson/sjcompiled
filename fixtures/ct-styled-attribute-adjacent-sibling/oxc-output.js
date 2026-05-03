import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _ = "._jus61qr7 [data-field]+button{min-width:8pc}";
const Container = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_jus61qr7", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Container.displayName = "Container";
}
export const Component = () => <Container>
    <span data-field />
    <button type="button">Action</button>
  </Container>;
