import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _3 = "._otyr7vkz{margin-bottom:1pc}";
const _2 = "._syaz143u{color:navy}";
const _ = "._1wybgktf{font-size:20px}";
const Heading = forwardRef(({ as: C = "h2", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1wybgktf _syaz143u _otyr7vkz", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Heading.displayName = "Heading";
}
