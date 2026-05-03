import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _5 = "._1wpz1fhb{align-self:stretch}";
const _4 = "._4cvr1h6o{align-items:center}";
const _3 = "._2lx21bp4{flex-direction:column}";
const _2 = "._1e0c1txw{display:flex}";
const _ = "._zulpopcn{gap:var(--space-200,4px)}";
const ModalBodyWrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_zulpopcn _1e0c1txw _2lx21bp4", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	ModalBodyWrapper.displayName = "ModalBodyWrapper";
}
const ProgressBarWrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_4,
		_5
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_zulpopcn _1e0c1txw _4cvr1h6o _1wpz1fhb", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	ProgressBarWrapper.displayName = "ProgressBarWrapper";
}
export const Example = ({ isAlternate }) => <div>
    <ModalBodyWrapper data-testid={isAlternate ? "alt" : "default"}>
      Content
    </ModalBodyWrapper>
    <ProgressBarWrapper>
      <span>Hello</span>
    </ProgressBarWrapper>
  </div>;
