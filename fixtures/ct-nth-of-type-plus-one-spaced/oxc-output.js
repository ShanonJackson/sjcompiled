import _React from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _ = "._1vykftgi:nth-of-type(n+1){padding-left:8px}";
// Reproduces spacing in nth-of-type selector that mismatches between Babel/SWC.
const statisticStyles = null;
const Fixture = () => <table>
		<tbody>
			<tr>
				<CC>
  <CS>{[_]}</CS>
  {<td className={ax(["_1vykftgi"])}>first</td>}
  </CC>
			</tr>
			<tr>
				<CC>
  <CS>{[_]}</CS>
  {<td className={ax(["_1vykftgi"])}>second</td>}
  </CC>
			</tr>
		</tbody>
	</table>;
export default Fixture;
