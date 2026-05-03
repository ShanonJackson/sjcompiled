import React from "react";
import { cx } from "@compiled/react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _11 = "@media (min-width:64rem){._10y41txw._10y41txw{display:flex}}";
const _10 = "._vchh1ntv{box-sizing:content-box}";
const _9 = "._p12fnklw{max-width:20pc}";
const _8 = "._1e0cglyw{display:none}";
const _7 = "._18zru2gc{padding-inline:var(--ds-space-100,8px)}";
const _6 = "._1di69yc7:active{background-color:var(--ds-background-neutral-subtle-pressed,#0b120e24)!important}";
const _5 = "._irr31dpa:hover{background-color:var(--ds-background-neutral-subtle-hovered,#0515240f)}";
const _4 = "._4t3izwfg{height:2pc}";
const _3 = "._4cvr1h6o{align-items:center}";
const _2 = "._1e0c1txw{display:flex}";
const _ = "._2rkofajl{border-radius:var(--ds-radius-small,3px)}";
const anchorStyles = {
	root: "_2rkofajl _1e0c1txw _4cvr1h6o _4t3izwfg",
	newInteractionStates: "_irr31dpa _1di69yc7"
};
const logoContainerStyles = { root: "_18zru2gc _1e0cglyw _p12fnklw _vchh1ntv _10y41txw" };
const LogoRenderer = ({ logoOrIcon }) => {
	return <div>{logoOrIcon}</div>;
};
const Anchor = ({ children, xcss, ...props }) => {
	return <a {...props}>{children}</a>;
};
export const CustomLogo = ({ href, logo, icon, onClick, label }) => {
	return <CC>
  <CS>{[
		_,
		_2,
		_3,
		_4,
		_5,
		_6
	]}</CS>
  {<Anchor aria-label={label} href={href} xcss={cx(anchorStyles.root, anchorStyles.newInteractionStates)} onClick={onClick}>
      <CC>
  <CS>{[
		_7,
		_8,
		_9,
		_10,
		_11
	]}</CS>
  {<div className={ax([logoContainerStyles.root])}>
        <LogoRenderer logoOrIcon={logo} />
      </div>}
  </CC>
    </Anchor>}
  </CC>;
};
