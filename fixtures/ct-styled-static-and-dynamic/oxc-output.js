import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _3 = "._1itkglyw{background-image:none}";
const _2 = "._1itkikq0{background-image:var(--_wrma1b)}";
const _ = "._i0dlexct{flex-basis:16px}";
const SIZE = 16;
const Icon = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { url, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_wrma1b": ix(__cmplp.url, ")", "url(")
	}} ref={__cmplr} className={ax([
		"_i0dlexct",
		__cmplp.url ? "_1itkikq0" : "_1itkglyw",
		__cmplp.className
	])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Icon.displayName = "Icon";
}
export const Component = ({ url }) => <Icon url={url} />;
