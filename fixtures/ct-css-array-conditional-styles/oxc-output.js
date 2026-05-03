import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _3 = "._19pk10j7{margin-top:-40px}";
const _2 = "._uwhkftgi{border-top-width:8px}";
const _ = "._p12f107j{max-width:20in}";
const fullpageStyles = null;
const modalStyles = null;
const customSpacingStyles = null;
export default function Component({ isEmbedView, isModalView }) {
	const customCss = [
		customSpacingStyles,
		isEmbedView !== true && isModalView !== true ? fullpageStyles : null,
		isModalView === true ? modalStyles : null
	];
	return <CC>
  <CS>{[
		_,
		_2,
		_3
	]}</CS>
  {<div className={ax([
		"_p12f107j",
		isEmbedView !== true && isModalView !== true && "_uwhkftgi",
		isModalView === true && "_19pk10j7"
	])}>hello</div>}
  </CC>;
}
