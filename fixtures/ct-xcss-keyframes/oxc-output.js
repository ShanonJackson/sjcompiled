import React from "react";
import { xcss } from "@atlaskit/primitives";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const shimmer = null;
const styles = xcss({
	width: "100%",
	animation: `${shimmer} 1s infinite`,
	background: "red"
});
export const Component = () => <div xcss={styles} />;
