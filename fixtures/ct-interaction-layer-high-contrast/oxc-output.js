import React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _22 = "@supports (-ms-ime-align:auto){._bwdwb3bt:focus:after{content:\"\"}._6ycq1ule:focus:after{display:block}._1h9dstnw:focus:after{position:absolute}._i6g0kb7n:focus:after{z-index:1}._9jtdidpf:focus:after{left:0}._1snmidpf:focus:after{right:0}._1jstidpf:focus:after{top:0}._pnw9idpf:focus:after{bottom:0}._xp6a1y54:focus:after{box-shadow:0 0 0 2px var(--ds-border-focused,#4688ec)}._15y0glyw:focus:after{pointer-events:none}._xp6a2ef9:focus:after{box-shadow:inset 0 0 0 2px var(--ds-border-focused,#4688ec)}}";
const _21 = "@media (-ms-high-contrast:none),screen and (-ms-high-contrast:active){._zjd3b3bt:focus:after{content:\"\"}._1vfc1ule:focus:after{display:block}._hkspstnw:focus:after{position:absolute}._1hl4kb7n:focus:after{z-index:1}._ehfnidpf:focus:after{left:0}._1btaidpf:focus:after{right:0}._1vdbidpf:focus:after{top:0}._o6k0idpf:focus:after{bottom:0}._1m551y54:focus:after{box-shadow:0 0 0 2px var(--ds-border-focused,#4688ec)}._1191glyw:focus:after{pointer-events:none}._1m552ef9:focus:after{box-shadow:inset 0 0 0 2px var(--ds-border-focused,#4688ec)}}";
const _20 = "._6q10glyw:focus:after{pointer-events:none}";
const _19 = "._1n511y54:focus:after{box-shadow:0 0 0 2px var(--ds-border-focused,#4688ec)}";
const _18 = "._gabdidpf:focus:after{bottom:0}";
const _17 = "._44x7idpf:focus:after{top:0}";
const _16 = "._ti30idpf:focus:after{right:0}";
const _15 = "._1a07idpf:focus:after{left:0}";
const _14 = "._1v98kb7n:focus:after{z-index:1}";
const _13 = "._1tfxstnw:focus:after{position:absolute}";
const _12 = "._j7gt1ule:focus:after{display:block}";
const _11 = "._h10pb3bt:focus:after{content:\"\"}";
const _10 = "._1lztewfl:focus~[data-component-selector=\"software-backlog.card-list.card.card-contents.context-menu.menu_placeholder\"]{visibility:visible}";
const _9 = "._q5c0kb7n:focus~[data-component-selector=\"software-backlog.card-list.card.card-contents.context-menu.menu_placeholder\"]{opacity:1}";
const _8 = "._1hvw1o36:focus{outline-width:medium}";
const _7 = "._49pcglyw:focus{outline-style:none}";
const _6 = "._nt751r31:focus{outline-color:currentColor}";
const _5 = "._1xi2idpf{right:0}";
const _4 = "._1ltvidpf{left:0}";
const _3 = "._94n5idpf{bottom:0}";
const _2 = "._154iidpf{top:0}";
const _ = "._kqswstnw{position:absolute}";
const MENU_PLACEHOLDER_ID = "software-backlog.card-list.card.card-contents.context-menu.menu_placeholder";
const cardFocusStyles = {
	content: "",
	display: "block",
	position: "absolute",
	zIndex: 1,
	left: 0,
	right: 0,
	top: 0,
	bottom: 0,
	boxShadow: `0 0 0 2px ${"var(--ds-border-focused, #4688EC)"}`,
	pointerEvents: "none"
};
const Container = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
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
		_10,
		_11,
		_12,
		_13,
		_14,
		_15,
		_16,
		_17,
		_18,
		_19,
		_20,
		_21,
		_22
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_kqswstnw _154iidpf _94n5idpf _1ltvidpf _1xi2idpf _nt751r31 _49pcglyw _1hvw1o36 _q5c0kb7n _1lztewfl _h10pb3bt _j7gt1ule _1tfxstnw _1v98kb7n _1a07idpf _ti30idpf _44x7idpf _gabdidpf _1n511y54 _6q10glyw _zjd3b3bt _1vfc1ule _hkspstnw _1hl4kb7n _ehfnidpf _1btaidpf _1vdbidpf _o6k0idpf _1m551y54 _1191glyw _1m552ef9 _bwdwb3bt _6ycq1ule _1h9dstnw _i6g0kb7n _9jtdidpf _1snmidpf _1jstidpf _pnw9idpf _xp6a1y54 _15y0glyw _xp6a2ef9", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Container.displayName = "Container";
}
const Fixture = () => <Container data-testid="interaction-layer" />;
export default Fixture;
