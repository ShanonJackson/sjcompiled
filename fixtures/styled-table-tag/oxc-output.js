import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _13 = "._10l91dcg td{border-bottom:1px solid #eee}";
const _12 = "._1pqwftgi td{padding-left:8px}";
const _11 = "._6autftgi td{padding-bottom:8px}";
const _10 = "._owipftgi td{padding-right:8px}";
const _9 = "._p4liftgi td{padding-top:8px}";
const _8 = "._xhev1e5h th{text-align:left}";
const _7 = "._1m2z1dht th{background-color:#f0f0f0}";
const _6 = "._il49ftgi th{padding-left:8px}";
const _5 = "._lxprftgi th{padding-bottom:8px}";
const _4 = "._1160ftgi th{padding-right:8px}";
const _3 = "._1fr8ftgi th{padding-top:8px}";
const _2 = "._yq5hcfaq{border-collapse:collapse}";
const _ = "._1bsb1osq{width:100%}";
const StyledTable = forwardRef(({ as: C = "table", style: __cmpls, ...__cmplp }, __cmplr) => {
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
		_13
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1bsb1osq _yq5hcfaq _1fr8ftgi _1160ftgi _lxprftgi _il49ftgi _1m2z1dht _xhev1e5h _p4liftgi _owipftgi _6autftgi _1pqwftgi _10l91dcg", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	StyledTable.displayName = "StyledTable";
}
