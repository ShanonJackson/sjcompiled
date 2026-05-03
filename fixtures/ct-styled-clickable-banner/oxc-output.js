import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _10 = "._11kduvh6:active [data-component-selector=shortcut-icon-lQi8]{color:selected}";
const _9 = "._j6xt1euz:active{background:var(--_1yb3cim)}";
const _8 = "._c2zzuvh6:hover [data-component-selector=shortcut-icon-lQi8]{color:selected}";
const _7 = "._xvyzuvh6:focus [data-component-selector=shortcut-icon-lQi8]{color:selected}";
const _6 = "._19lcm6ea:hover{background:var(--_1qak2ff)}";
const _5 = "._1du2m6ea:focus{background:var(--_1qak2ff)}";
const _4 = "._80om87zm{cursor:var(--_y6477n)}";
const _3 = "._1rjcftgi{padding-block:8px}";
const _2 = "._18zr10ul{padding-inline:var(--_p92on0)}";
const _ = "._11q71qmv{background:var(--_uotol0)}";
const containerBackgrounds = {
	information: {
		default: "info-default",
		hovered: "info-hovered",
		active: "info-active"
	},
	warning: {
		default: "warn-default",
		hovered: "warn-hovered",
		active: "warn-active"
	}
};
const Container = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { hasAction, spacing, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4,
		_5,
		_6,
		_7,
		_8,
		_9,
		_10
	]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_y6477n": ix(__cmplp.hasAction && 'pointer'),
		"--_uotol0": ix(containerBackgrounds[__cmplp.type].default),
		"--_p92on0": ix(__cmplp.spacing),
		"--_1qak2ff": ix(containerBackgrounds[__cmplp.type].hovered),
		"--_1yb3cim": ix(containerBackgrounds[__cmplp.type].active)
	}} ref={__cmplr} className={ax(["_11q71qmv _18zr10ul _1rjcftgi _80om87zm _1du2m6ea _19lcm6ea _xvyzuvh6 _c2zzuvh6 _j6xt1euz _11kduvh6", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Container.displayName = "Container";
}
export const Component = ({ hasAction = true, type = "information", spacing = "16px" }) => <Container hasAction={hasAction} type={type} spacing={spacing}>
		Content
		<span data-component-selector="shortcut-icon-lQi8">icon</span>
	</Container>;
