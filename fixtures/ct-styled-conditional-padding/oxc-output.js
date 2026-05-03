import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _5 = "._19bv1crf{padding-left:9pt}";
const _4 = "._19bvidpf{padding-left:0}";
const _3 = "._n3tdftgi{padding-bottom:8px}";
const _2 = "._u5f3ftgi{padding-right:8px}";
const _ = "._ca0q1y44{padding-top:4px}";
const PaddingWrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { isSummaryView, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4,
		_5
	]}</CS>
        <C {...__cmpldp} style={__cmpls} ref={__cmplr} className={ax([
		"",
		__cmplp.isSummaryView ? "_ca0q1y44 _u5f3ftgi _n3tdftgi _19bvidpf" : "_ca0q1y44 _u5f3ftgi _n3tdftgi _19bv1crf",
		__cmplp.className
	])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	PaddingWrapper.displayName = "PaddingWrapper";
}
export const Component = () => <PaddingWrapper isSummaryView={false}>Content</PaddingWrapper>;
