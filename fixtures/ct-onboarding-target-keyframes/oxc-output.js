import _React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _7 = "@media (prefers-reduced-motion:reduce){._1bumglyw{animation:none}._sedtglyw{transition:none}}";
const _6 = "@keyframes kjdbdy5{0%{box-shadow:0 0 0 2px #6554c0,0 0 0 #6554c0}33%{box-shadow:0 0 0 2px #6554c0,0 0 0 #6554c0}66%{box-shadow:0 0 0 2px #6554c0,0 0 0 10px rgba(101,84,192,.01)}to{box-shadow:0 0 0 2px #6554c0,0 0 0 10px rgba(101,84,192,.01)}}";
const _5 = "._16qsezjf{box-shadow:0 0 0 2px #6554c0}";
const _4 = "._1pglmcjr{animation-timing-function:cubic-bezier(.55,.055,.675,.19)}";
const _3 = "._j7hq1942{animation-name:kjdbdy5}";
const _2 = "._tip812c5{animation-iteration-count:infinite}";
const _ = "._5sagi11n{animation-duration:3s}";
const baseShadow = "0 0 0 2px #6554C0";
const easing = "cubic-bezier(0.55, 0.055, 0.675, 0.19)";
const pulseKeyframes = null;
const reduceMotionAsPerUserPreference = null;
const animationStyles = null;
export const Pulse = ({ children, pulse = true, ...props }) => <CC>
  <CS>{[
	_,
	_2,
	_3,
	_4,
	_5,
	_6,
	_7
]}</CS>
  {<div {...props} className={ax([pulse && "_5sagi11n _tip812c5 _j7hq1942 _1pglmcjr _16qsezjf", "_1bumglyw _sedtglyw"])}>
		{children}
	</div>}
  </CC>;
