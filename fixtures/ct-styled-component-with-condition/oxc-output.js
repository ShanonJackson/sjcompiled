import { componentWithCondition } from "@atlassian/jira-feature-flagging-utils";
import { easeInOut } from "@atlaskit/motion";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _7 = "._1o9zidpf{flex-shrink:0}";
const _6 = "._18m915vq{overflow-y:hidden}";
const _5 = "._1reo15vq{overflow-x:hidden}";
const _4 = "._1bsb1rkg{width:var(--_1gljcou)}";
const _3 = "._njlp1rql{contain:layout}";
const _2 = "._1e0c1txw{display:flex}";
const _ = "._v5641lsu{transition:var(--_7nn7wk)}";
const OuterWrapperOld = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { duration, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4
	]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_1gljcou": ix(__cmplp.width),
		"--_7nn7wk": ix(`width ${__cmplp.duration}ms ${easeInOut}`)
	}} ref={__cmplr} className={ax(["_v5641lsu _1e0c1txw _njlp1rql _1bsb1rkg", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	OuterWrapperOld.displayName = "OuterWrapperOld";
}
const OuterWrapperNew = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { duration, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_5,
		_6,
		_2,
		_3,
		_4
	]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_1gljcou": ix(__cmplp.width),
		"--_7nn7wk": ix(`width ${__cmplp.duration}ms ${easeInOut}`)
	}} ref={__cmplr} className={ax(["_v5641lsu _1reo15vq _18m915vq _1e0c1txw _njlp1rql _1bsb1rkg", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	OuterWrapperNew.displayName = "OuterWrapperNew";
}
const OuterWrapper = componentWithCondition(() => true, OuterWrapperNew, OuterWrapperOld);
const InnerWrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_7]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1o9zidpf", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	InnerWrapper.displayName = "InnerWrapper";
}
export const Component = ({ width, duration }) => <OuterWrapper width={width} duration={duration}>
    <InnerWrapper />
  </OuterWrapper>;
