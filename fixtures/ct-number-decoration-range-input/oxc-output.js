import Textfield from "@atlaskit/textfield";
import _React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _2 = "._syazi7uo{color:var(--ds-text,#292a2e)}";
const _ = "._syaz1tmw{color:var(--ds-text-danger,#ae2e24)}";
const textFieldStyle = {
	invalid: "_syaz1tmw",
	valid: "_syazi7uo"
};
export const Example = ({ isValid }) => <CC>
  <CS>{[_, _2]}</CS>
  {<Textfield className={ax([])} />}
  </CC>;
