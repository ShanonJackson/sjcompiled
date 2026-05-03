import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _8 = "._syaz1iu8{color:#daa520}";
const _7 = "._1yt4puy2{padding:var(--_1oru8t5)}";
const _6 = "@keyframes k1bdgii{0%{opacity:0}to{opacity:1}}";
const _5 = "._1llw120f:hover{transform:scale(1.1)}";
const _4 = "._1h6d1iu8{border-color:#daa520}";
const _3 = "._1dqonqa1{border-style:solid}";
const _2 = "._189eyh40{border-width:2px}";
const _ = "._y44v6l02{animation:k1bdgii 2s ease-in-out}";
const colors = {
	primary: "tomato",
	secondary: "#daa520"
};
const fadeIn = null;
export const dynamicText = null;
export const FancyButton = forwardRef(({ as: C = "button", style: __cmpls, ...__cmplp }, __cmplr) => {
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
		_6
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_y44v6l02 _189eyh40 _1dqonqa1 _1h6d1iu8 _1llw120f", __cmplp.className])} />
      </CC>;
});
const themed = {
	primary: "_syaz1a6z",
	secondary: "_syaz1iu8"
};
const alias = "secondary";
export const mappedClass = themed[alias];
export const WithClassNames = () => <CC>
  <CS>{[_7, _8]}</CS>
  {<div className={ax(["_1yt4puy2 _syaz1iu8"])}>
        example
      </div>}
  </CC>;
if (process.env.NODE_ENV !== "production") {
	FancyButton.displayName = "FancyButton";
}
