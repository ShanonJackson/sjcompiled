import React, { useState, useMemo, useCallback, useContext, createContext } from "react";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
const _50 = "._syaz9sh9{color:#6b778c}";
const _49 = "._1wyb1crf{font-size:9pt}";
const _48 = "._1bah1yb4{justify-content:space-between}";
const _47 = "._1bg4yh40:focus{outline-offset:2px}";
const _46 = "._1hvwyh40:focus{outline-width:2px}";
const _45 = "._49pcnqa1:focus{outline-style:solid}";
const _44 = "._nt75b0wy:focus{outline-color:#4c9aff}";
const _43 = "._vwz4yu22{line-height:1.6}";
const _42 = "._1wybo7ao{font-size:15px}";
const _41 = "._bfhku67f{background-color:#fff}";
const _40 = "._1tkeuuw1{min-height:200px}";
const _39 = "._19it1i89{border:1px solid #dfe1e6}";
const _38 = "._2rko1crf{border-radius:9pt}";
const _37 = "._19bv7vkz{padding-left:1pc}";
const _36 = "._n3td7vkz{padding-bottom:1pc}";
const _35 = "._u5f37vkz{padding-right:1pc}";
const _34 = "._ca0q7vkz{padding-top:1pc}";
const _33 = "._4rtxtlke button{cursor:pointer}";
const _32 = "._7dir1j28 button{background-color:transparent}";
const _31 = "._1i49angw button{text-transform:uppercase}";
const _30 = "._1o7p1crf button{font-size:9pt}";
const _29 = "._1uegowjk button[data-active=true]{color:#7f5af0}";
const _28 = "._19e7owjk button[data-active=true]{border-color:#7f5af0}";
const _27 = "._6pupu67f button:hover{background-color:#fff}";
const _26 = "._m4w23739 button:hover{border-color:#dfe1e6}";
const _25 = "._11lvftgi button{padding-left:8px}";
const _24 = "._1gqn1y44 button{padding-bottom:4px}";
const _23 = "._yhjmftgi button{padding-right:8px}";
const _22 = "._1gfl1y44 button{padding-top:4px}";
const _21 = "._zs121y44 button{border-radius:4px}";
const _20 = "._stfgokh7 button{border:1px solid transparent}";
const _19 = "._1n261g80{flex-wrap:wrap}";
const _18 = "._1e0c1txw{display:flex}";
const _17 = "._zulpftgi{gap:8px}";
const _16 = "._bfhk1hxd{background-color:#f4f5f7}";
const _15 = "._1e0c11p5{display:grid}";
const _14 = "._2rko7vkz{border-radius:1pc}";
const _13 = "._19bv1tcg{padding-left:24px}";
const _12 = "._n3td1tcg{padding-bottom:24px}";
const _11 = "._u5f31tcg{padding-right:24px}";
const _10 = "._ca0q1tcg{padding-top:24px}";
const _9 = "._zulp7vkz{gap:1pc}";
const _8 = "._1p1dangw{text-transform:uppercase}";
const _7 = "._1wyb19bv{font-size:10px}";
const _6 = "._19itx2ht{border:1px solid currentColor}";
const _5 = "._2rkow7q6{border-radius:999px}";
const _4 = "._19bvi2wt{padding-left:6px}";
const _3 = "._n3tdyh40{padding-bottom:2px}";
const _2 = "._u5f3i2wt{padding-right:6px}";
const _ = "._ca0qyh40{padding-top:2px}";
const EditorThemeContext = createContext({ mode: "light" });
const containerStyles = null;
const toolbarStyles = null;
const editorStyles = null;
const statusStyles = null;
const badgeStyles = null;
const ToneBadge = ({ mode }) => <CC>
  <CS>{[
	_,
	_2,
	_3,
	_4,
	_5,
	_6,
	_7,
	_8
]}</CS>
  {<span style={{ color: mode === "dark" ? "#9BE7FF" : "#1D7AFC" }} className={ax(["_ca0qyh40 _u5f3i2wt _n3tdyh40 _19bvi2wt _2rkow7q6 _19itx2ht _1wyb19bv _1p1dangw"])}>
    {mode} mode
  </span>}
  </CC>;
const ToolbarButton = ({ active, label, onClick }) => <button type="button" data-active={active} onClick={onClick}>
    {label}
  </button>;
const EditorShell = () => {
	const [value, setValue] = useState("Type something beautiful…");
	const [bold, setBold] = useState(false);
	const [italics, setItalics] = useState(false);
	const [mode, setMode] = useState("light");
	const wordCount = useMemo(() => value.trim().split(/\s+/).filter(Boolean).length, [value]);
	const handleToggle = useCallback((setter) => () => setter((prev) => !prev), []);
	const contextValue = useMemo(() => ({
		mode,
		toggleMode: () => setMode((prev) => prev === "light" ? "dark" : "light")
	}), [mode]);
	return <EditorThemeContext.Provider value={contextValue}>
      <CC>
  <CS>{[
		_9,
		_10,
		_11,
		_12,
		_13,
		_14,
		_15,
		_16
	]}</CS>
  {<section aria-label="Rich text editor" className={ax(["_zulp7vkz _ca0q1tcg _u5f31tcg _n3td1tcg _19bv1tcg _2rko7vkz _1e0c11p5 _bfhk1hxd"])}>
        <CC>
  <CS>{[
		_17,
		_18,
		_19,
		_20,
		_21,
		_22,
		_23,
		_24,
		_25,
		_26,
		_27,
		_28,
		_29,
		_30,
		_31,
		_32,
		_33
	]}</CS>
  {<header className={ax(["_zulpftgi _1e0c1txw _1n261g80 _stfgokh7 _zs121y44 _1gfl1y44 _yhjmftgi _1gqn1y44 _11lvftgi _m4w23739 _6pupu67f _19e7owjk _1uegowjk _1o7p1crf _1i49angw _7dir1j28 _4rtxtlke"])}>
          <ToolbarButton active={bold} label="Bold" onClick={handleToggle(setBold)} />
          <ToolbarButton active={italics} label="Italics" onClick={handleToggle(setItalics)} />
          <ToolbarButton active={mode === "dark"} label="Toggle theme" onClick={() => contextValue.toggleMode()} />
        </header>}
  </CC>
        <CC>
  <CS>{[
		_34,
		_35,
		_36,
		_37,
		_38,
		_39,
		_40,
		_41,
		_42,
		_43,
		_44,
		_45,
		_46,
		_47
	]}</CS>
  {<textarea style={{
		fontWeight: bold ? 600 : 400,
		fontStyle: italics ? "italic" : "normal"
	}} value={value} onChange={(event) => setValue(event.target.value)} className={ax(["_ca0q7vkz _u5f37vkz _n3td7vkz _19bv7vkz _2rko1crf _19it1i89 _1tkeuuw1 _bfhku67f _1wybo7ao _vwz4yu22 _nt75b0wy _49pcnqa1 _1hvwyh40 _1bg4yh40"])} />}
  </CC>
        <CC>
  <CS>{[
		_18,
		_48,
		_49,
		_50
	]}</CS>
  {<footer className={ax(["_1e0c1txw _1bah1yb4 _1wyb1crf _syaz9sh9"])}>
          <span>{wordCount} words</span>
          <EditorStatus />
        </footer>}
  </CC>
      </section>}
  </CC>
    </EditorThemeContext.Provider>;
};
const EditorStatus = () => {
	const { mode } = useContext(EditorThemeContext);
	return <ToneBadge mode={mode} />;
};
export const Component = () => <EditorShell />;
