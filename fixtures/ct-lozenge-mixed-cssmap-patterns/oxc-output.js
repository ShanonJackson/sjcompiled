import _React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _19 = "._1dyz9vsi{letter-spacing:.165px}";
const _18 = "._o5721q9c{white-space:nowrap}";
const _17 = "._1p1dangw{text-transform:uppercase}";
const _16 = "._1bto1l2s{text-overflow:ellipsis}";
const _15 = "._vwz47vkz{line-height:1pc}";
const _14 = "._k48pmoej{font-weight:var(--ds-font-weight-bold,700)}";
const _13 = "._zg8l4jg8{font-style:normal}";
const _12 = "._1wyb1skh{font-size:11px}";
const _11 = "._ect41gqc{font-family:var(--ds-font-family-body,ui-sans-serif,-apple-system,BlinkMacSystemFont,\"Segoe UI\",Ubuntu,\"Helvetica Neue\",sans-serif)}";
const _10 = "._syazazsu{color:var(--ds-text-subtle,#505258)}";
const _9 = "._syaz15cr{color:var(--ds-text-inverse,#fff)}";
const _8 = "._vchhusvi{box-sizing:border-box}";
const _7 = "._kqswpfqs{position:static}";
const _6 = "._1kz6184x{block-size:-moz-min-content;block-size:min-content}";
const _5 = "._1e0c116y{display:inline-flex}";
const _4 = "._18zr1b66{padding-inline:var(--ds-space-050,4px)}";
const _3 = "._18m915vq{overflow-y:hidden}";
const _2 = "._1reo15vq{overflow-x:hidden}";
const _ = "._2rko12b0{border-radius:var(--ds-radius-small,4px)}";
const stylesOld = {
	container: "_2rko12b0 _1reo15vq _18m915vq _18zr1b66 _1e0c116y _1kz6184x _kqswpfqs _vchhusvi",
	"text.bold.default": "_syaz15cr",
	"text.bold.inprogress": "_syaz15cr",
	"text.subtle.default": "_syazazsu"
};
const stylesOldUnbounded = {
	text: "_1reo15vq _18m915vq _ect41gqc _1wyb1skh _zg8l4jg8 _k48pmoej _vwz47vkz _1bto1l2s _1p1dangw _o5721q9c",
	customLetterspacing: "_1dyz9vsi"
};
export const Lozenge = ({ children, appearance = "default", isBold = false }) => <CC>
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
	_19
]}</CS>
  {<div className={ax([
	stylesOld.container,
	stylesOldUnbounded.text,
	stylesOldUnbounded.customLetterspacing
])}>
		<CC>
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
  {<span className={ax([stylesOld[`text.${isBold ? "bold" : "subtle"}.${appearance}`]])}>
			{children}
		</span>}
  </CC>
	</div>}
  </CC>;
