import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _21 = "._925w1h6o >button>span{align-self:center}";
const _20 = "._1kne1nzx >button:hover{height:105px}";
const _19 = "._r6ob1nzx >button:disabled{height:105px}";
const _18 = "._1p4f1nzx >:hover{height:105px}";
const _17 = "._1u1q1nzx >:disabled{height:105px}";
const _16 = "._t74o1nzx >button{height:105px}";
const _15 = "._wvzr1nzx >*{height:105px}";
const _14 = "._15h6idpf >button{margin-left:0}";
const _13 = "._1ko9idpf >*{margin-left:0}";
const _12 = "._2gv614y2 >button{margin-bottom:5px}";
const _11 = "._8jx714y2 >*{margin-bottom:5px}";
const _10 = "._1ilridpf >button{margin-right:0}";
const _9 = "._d4l7idpf >*{margin-right:0}";
const _8 = "._1v09idpf >button{margin-top:0}";
const _7 = "._1mizidpf >*{margin-top:0}";
const _6 = "._1yp31ssb >button{flex-basis:50%}";
const _5 = "._osiy1ssb >*{flex-basis:50%}";
const _4 = "._cs56idpf >button{flex-shrink:0}";
const _3 = "._7rs5idpf >*{flex-shrink:0}";
const _2 = "._hxt6idpf >button{flex-grow:0}";
const _ = "._12apidpf >*{flex-grow:0}";
const QuarterPickerContainer = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
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
		_21
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_12apidpf _hxt6idpf _7rs5idpf _cs56idpf _osiy1ssb _1yp31ssb _1mizidpf _1v09idpf _d4l7idpf _1ilridpf _8jx714y2 _2gv614y2 _1ko9idpf _15h6idpf _wvzr1nzx _t74o1nzx _1u1q1nzx _1p4f1nzx _r6ob1nzx _1kne1nzx _925w1h6o", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	QuarterPickerContainer.displayName = "QuarterPickerContainer";
}
const QuarterPicker = () => <QuarterPickerContainer>
    <button>
      <span>Custom</span>
    </button>
    <div>Quarter</div>
  </QuarterPickerContainer>;
export default QuarterPicker;
