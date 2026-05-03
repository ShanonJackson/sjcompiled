import { gridSize } from "./grid-size";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _5 = "._1bto1l2s{text-overflow:ellipsis}";
const _4 = "._o5721q9c{white-space:nowrap}";
const _3 = "._2hwx1tcg{margin-right:24px}";
const _2 = "._18m915vq{overflow-y:hidden}";
const _ = "._1reo15vq{overflow-x:hidden}";
const nameStyles = `
  margin-right: ${gridSize * 3}px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
`;
const NameLink = forwardRef(({ as: C = "a", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4,
		_5
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_1reo15vq _18m915vq _2hwx1tcg _o5721q9c _1bto1l2s", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	NameLink.displayName = "NameLink";
}
export const Component = () => <NameLink>text</NameLink>;
