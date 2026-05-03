import _React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _7 = "@media (prefers-reduced-motion:reduce){._1bumglyw{animation:none}._sedtglyw{transition:none}}";
const _6 = "@keyframes ke0nii9{0%{box-shadow:0 0 0 2px var(--ds-border-discovery,#af59e1),0 0 0 var(--ds-border-discovery,#af59e1)}33%{box-shadow:0 0 0 2px var(--ds-border-discovery,#af59e1),0 0 0 var(--ds-border-discovery,#af59e1)}66%{box-shadow:0 0 0 2px var(--ds-border-discovery,#af59e1),0 0 0 10px rgba(101,84,192,.01)}to{box-shadow:0 0 0 2px var(--ds-border-discovery,#af59e1),0 0 0 10px rgba(101,84,192,.01)}}";
const _5 = "._16qs1peg{box-shadow:0 0 0 2px var(--ds-border-discovery,#af59e1)}";
const _4 = "._1pglmcjr{animation-timing-function:cubic-bezier(.55,.055,.675,.19)}";
const _3 = "._j7hq1mi7{animation-name:ke0nii9}";
const _2 = "._tip812c5{animation-iteration-count:infinite}";
const _ = "._5sagi11n{animation-duration:3s}";
const reduceMotionAsPerUserPreference = null;
const baseShadow = `0 0 0 2px ${"var(--ds-border-discovery, #AF59E1)"}`;
const easing = "cubic-bezier(0.55, 0.055, 0.675, 0.19)";
const pulseKeyframes = null;
const animationStyles = null;
const Base = ({ bgColor, children, className, radius, testId, style, ...props }) => <div className={className} data-testid={testId} style={{
	...style,
	backgroundColor: bgColor,
	borderRadius: radius ? `${radius}px` : undefined
}} {...props}>
    {children}
  </div>;
export const TargetInner = ({ bgColor, children, className, pulse, radius, testId, ...props }) => <CC>
  <CS>{[
	_,
	_2,
	_3,
	_4,
	_5,
	_6,
	_7
]}</CS>
  {<Base bgColor={bgColor} radius={radius} testId={testId} {...props} style={props.style} className={ax([
	pulse && "_5sagi11n _tip812c5 _j7hq1mi7 _1pglmcjr _16qs1peg",
	"_1bumglyw _sedtglyw",
	className
])}>
    {children}
  </Base>}
  </CC>;
