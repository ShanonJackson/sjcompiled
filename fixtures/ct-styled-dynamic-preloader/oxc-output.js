import React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _6 = "._1tkerc84{min-height:var(--_1eqrisy)}";
const _5 = "._4cvr1h6o{align-items:center}";
const _4 = "._1bah1h6o{justify-content:center}";
const _3 = "._1e0c1txw{display:flex}";
const _2 = "._18m915vq{overflow-y:hidden}";
const _ = "._1reo15vq{overflow-x:hidden}";
const Preloader = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { hideLabel, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4,
		_5,
		_6
	]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_1eqrisy": ix(__cmplp.hideLabel ? '200px' : '120px')
	}} ref={__cmplr} className={ax(["_1reo15vq _18m915vq _1e0c1txw _1bah1h6o _4cvr1h6o _1tkerc84", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Preloader.displayName = "Preloader";
}
export const Component = ({ hideLabel }) => <Preloader hideLabel={hideLabel}>
    <div>content</div>
  </Preloader>;
