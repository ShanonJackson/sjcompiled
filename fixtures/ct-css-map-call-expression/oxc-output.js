import * as styles from "./styles.module.css";
import _React, { forwardRef } from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _ = "._1lmc1r3e{grid-template-areas:var(--_o48r1v)}";
const gridAreas = () => {
	if (styles.primary && styles.tertiary && styles.secondary) {
		return `"${styles.primary} ${styles.tertiary} ${styles.secondary}"`;
	}
	return "\"primary tertiary secondary\"";
};
const Footer = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_]}</CS>
        <C {...__cmplp} style={{
		...__cmpls,
		"--_o48r1v": ix(gridAreas())
	}} ref={__cmplr} className={ax(["_1lmc1r3e", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	Footer.displayName = "Footer";
}
export const Component = () => <Footer />;
