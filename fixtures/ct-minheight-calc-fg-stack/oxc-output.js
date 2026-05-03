import { fg } from "@atlassian/jira-feature-gating";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _2 = "._1tkei63v{min-height:calc(var(--board-scroll-element-height)*1px - 8px)}";
const _ = "._1tke1osq{min-height:100%}";
const Comp = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { isFlexible, isSwimlaneMode, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[_, _2]}</CS>
        <C {...__cmpldp} style={__cmpls} ref={__cmplr} className={ax([
		"",
		__cmplp.isFlexible && !__cmplp.isSwimlaneMode && (fg("avoid_board_scroll_container_style_changes") ? "_1tke1osq" : "_1tkei63v"),
		__cmplp.className
	])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Comp.displayName = "Comp";
}
export const Component = () => <Comp isFlexible isSwimlaneMode={false} />;
