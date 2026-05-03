import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _7 = "._kqswstnw{position:absolute}";
const _6 = "._4t3i1b66{height:var(--ds-space-050,4px)}";
const _5 = "._1xi2idpf{right:0}";
const _4 = "._1ltvidpf{left:0}";
const _3 = "._154iidpf{top:0}";
const _2 = "._bfhk1gpm{background-color:var(--_1h75lj6)}";
const _ = "._2rko12c2{border-radius:var(--ds-radius-medium,6px) var(--ds-radius-medium,6px) 0 0}";
const STATUS = {
	TODO: "TODO",
	IN_PROGRESS: "IN_PROGRESS",
	DONE: "DONE",
	UNKNOWN: "UNKNOWN"
};
const STATUS_COLOR_MAP = {
	[STATUS.TODO]: "var(--ds-icon-accent-gray, #7D818A)",
	[STATUS.IN_PROGRESS]: "var(--ds-icon-accent-blue, #357DE8)",
	[STATUS.DONE]: "var(--ds-icon-accent-green, #22A06B)",
	[STATUS.UNKNOWN]: "unset"
};
export const StatusBar = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { status, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4,
		_5,
		_6,
		_7
	]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_1h75lj6": ix(STATUS_COLOR_MAP[__cmplp.status])
	}} ref={__cmplr} className={ax(["_2rko12c2 _bfhk1gpm _154iidpf _1ltvidpf _1xi2idpf _4t3i1b66 _kqswstnw", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	StatusBar.displayName = "StatusBar";
}
