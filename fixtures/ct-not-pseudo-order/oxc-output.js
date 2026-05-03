import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _2 = "._tideidpf:not(:hover,:focus-within)>[data-component=delete-question-button]{opacity:0}";
const _ = "._1brf1y44>[data-component=delete-question-button]{margin-right:4px}";
const Wrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_, _2]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1brf1y44 _tideidpf", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Wrapper.displayName = "Wrapper";
}
export const Example = () => <Wrapper>
    <button data-component="delete-question-button">Delete</button>
  </Wrapper>;
