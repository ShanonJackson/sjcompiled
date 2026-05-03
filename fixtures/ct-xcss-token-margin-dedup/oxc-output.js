import { cx } from "@atlaskit/css";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _6 = "._otyrv5ey{margin-bottom:var(--space-100,4px)}";
const _5 = "._18u0idpf{margin-left:0}";
const _4 = "._otyridpf{margin-bottom:0}";
const _3 = "._2hwxidpf{margin-right:0}";
const _2 = "._19pkidpf{margin-top:0}";
const _ = "._19pkv5ey{margin-top:var(--space-100,4px)}";
const DescriptionWrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_19pkv5ey", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	DescriptionWrapper.displayName = "DescriptionWrapper";
}
const TextWrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_2,
		_3,
		_4,
		_5
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_19pkidpf _2hwxidpf _otyridpf _18u0idpf", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	TextWrapper.displayName = "TextWrapper";
}
const styles = { bannerContainer: "_19pkv5ey _otyrv5ey" };
export const Component = ({ showBanner }) => <DescriptionWrapper>
    <TextWrapper>One</TextWrapper>
    {showBanner ? <CC>
  <CS>{[_, _6]}</CS>
  {<div xcss={cx(styles.bannerContainer)} />}
  </CC> : null}
  </DescriptionWrapper>;
