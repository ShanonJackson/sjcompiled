import React, { useMemo, useCallback, useState, useEffect, useRef, Component, memo } from "react";
import intersection from "lodash/intersection";
import isArray from "lodash/isArray";
import join from "lodash/join";
import round from "lodash/round";
import { combine } from "@atlaskit/pragmatic-drag-and-drop/combine";
import { draggable } from "@atlaskit/pragmatic-drag-and-drop/element/adapter";
import { N300, B500, N500, N0, N1000, B700 } from "@atlassian/jira-polaris-lib-color-palette/src/ui/colors/index.tsx";
import { fireUIAnalytics, useAnalyticsEvents } from "@atlassian/jira-product-analytics-bridge";
import { fg } from "@atlassian/jira-feature-gating";
import { DRAGGABLE_ITEM_TYPE } from "../../common/constants/index.tsx";
import { useMatrixActions } from "../../controllers/index.tsx";
import { useClusteredItemRendererComponent, useDraggableBubbleComponent, useItemRendererComponent } from "../../controllers/selectors/components-hooks.tsx";
import { useRelativeItemCenterPosition, useZAxisPositionForItems } from "../../controllers/selectors/positions-hooks.tsx";
import { useHoveredItems, useHighlightedItem, useSelectedItems } from "../../controllers/selectors/ui-hooks.tsx";
import { sendMatrixSingularDragOperationStarted, sendMatrixGroupDragOperationStarted, sendMatrixSingularDragOperationEnded, sendMatrixGroupDragOperationEnded, sendMatrixSingletonBubbleClicked, sendMatrixGroupBubbleClicked } from "../../services/pendo/index.tsx";
import { ax, ix, CC, CS } from "@compiled/react/runtime";
import { forwardRef } from "react";
const _45 = "._1pbycs5v{z-index:2}";
const _44 = "._1pby11wp{z-index:3}";
const _43 = "._v5641hpz{transition:.5s ease-in-out}";
const _42 = "._v56418uv{transition:initial}";
const _41 = "._17wj1kw7 div[role=presentation]{height:inherit}";
const _40 = "._4t3i1kw7{height:inherit}";
const _39 = "._syazt8es{color:var(--_1i123mx)}";
const _38 = "._syazfl7h{color:var(--_sjyp98)}";
const _37 = "._tzy4kb7n{opacity:1}";
const _36 = "._tzy4idpf{opacity:0}";
const _35 = "._d0altlke:hover{cursor:pointer}";
const _34 = "[data-component-selector=bubble-content-wrapper-phaV]:hover ._ejyi1nji{background-image:var(--_qk6zit)}";
const _33 = "[data-component-selector=bubble-content-wrapper-phaV]:hover ._eyzjklnc{background-color:var(--_t0t6fo)}";
const _32 = "._1yxgglyw>span{-webkit-user-select:none;-moz-user-select:none;user-select:none}";
const _31 = "._1y1m1pao{-webkit-background-clip:var(--_1xpu8xd);background-clip:var(--_1xpu8xd)}";
const _30 = "._1lrwwvw6{background-size:100% 100%,100% 100%}";
const _29 = "._bfhkwbqm{background-color:var(--_1mbry0z)}";
const _28 = "._1itknqgl{background-image:var(--_ur3htu)}";
const _27 = "._1bah1h6o{justify-content:center}";
const _26 = "._4cvr1h6o{align-items:center}";
const _25 = "._1e0c1txw{display:flex}";
const _24 = "._19itgqsr{border:var(--_1r7z21x)}";
const _23 = "._v5641bs3{transition:.5s}";
const _22 = "._11c8rymc{font:var(--ds-font-body-UNSAFE_small,normal 400 9pt/1pc \"Atlassian Sans\",ui-sans-serif,-apple-system,BlinkMacSystemFont,\"Segoe UI\",Ubuntu,\"Helvetica Neue\",sans-serif)}";
const _21 = "._1itkglyw{background-image:none}";
const _20 = "._1itk1wi1{background-image:var(--_4r9qlu)}";
const _19 = "[data-component-selector=bubble-content-wrapper-phaV]:hover ._nqoi12c5{animation-iteration-count:infinite}";
const _18 = "[data-component-selector=bubble-content-wrapper-phaV]:hover ._1canymdr{animation-duration:2s}";
const _17 = "[data-component-selector=bubble-content-wrapper-phaV]:hover ._15i11hgy{animation-name:k1q89j8a}";
const _16 = "@keyframes k1q89j8a{0%{transform:scale(1);opacity:.5}50%{left:-8px;top:-8px;height:calc(100% + 1pc);width:calc(100% + 1pc);opacity:.5}to{transform:scale(1);opacity:.5}}";
const _15 = "._1y1mxe2f{-webkit-background-clip:var(--_s99dce);background-clip:var(--_s99dce)}";
const _14 = "._qjkj9dsr{background-origin:content-box,border-box}";
const _13 = "._ouxl1eba{background-position:50% 50%,0 0}";
const _12 = "._12vemgnk{background-repeat:no-repeat}";
const _11 = "._1lrw174u{background-size:100% 100%}";
const _10 = "._bfhk10mk{background-color:var(--_1cid1vn)}";
const _9 = "._154iidpf{top:0}";
const _8 = "._1ltvidpf{left:0}";
const _7 = "._kqswstnw{position:absolute}";
const _6 = "._4t3i1osq{height:100%}";
const _5 = "._1bsb1osq{width:100%}";
const _4 = "._bfhk1j28{background-color:transparent}";
const _3 = "._kqswh2mm{position:relative}";
const _2 = "._vchhusvi{box-sizing:border-box}";
const _ = "._2rko1rr0{border-radius:var(--ds-radius-full,9999px)}";
// eslint-disable-next-line jira/react/no-class-components
class BubblePositioningComponent extends Component {
	state = { containerRef: undefined };
	shouldComponentUpdate(nextProps) {
		return nextProps.renderChildren !== this.props.renderChildren;
	}
	componentDidUpdate(prevProps) {
		if ((this.props.zPosition !== prevProps.zPosition || this.props.centerPosition !== prevProps.centerPosition) && this.state.containerRef) {
			this.updateStylesForProps(this.state.containerRef, this.props);
		}
	}
	updateStylesForProps = (ref, { zPosition, centerPosition }) => {
		if (ref) {
			const { style } = ref;
			style.width = `${zPosition}px`;
			style.height = `${zPosition}px`;
			style.left = `calc(${centerPosition.left}% - ${zPosition / 2}px)`;
			style.top = `calc(${centerPosition.top}% - ${zPosition / 2}px)`;
			// bucketed z positions are 30-45-60-75, assigning z index with inverse proportion
			// will make sure that smaller bubbles are always on top of bigger bubbles,
			// hence all of the bubbles will be visible
			style.zIndex = Math.ceil((75 - zPosition) / 15 + 2).toString();
		}
	};
	setContainerRef = (containerRef) => {
		this.setState({ containerRef });
		this.updateStylesForProps(containerRef, this.props);
	};
	render() {
		const { renderChildren, isHovered, isDragging } = this.props;
		return <BubblePositioningContainer isDragging={isDragging} ref={this.setContainerRef} isHovered={isHovered}>
				{renderChildren()}
			</BubblePositioningContainer>;
	}
}
export const DefaultBubbleComponent = ({ color, borderColor, isHovered, isSelected, isDragging, isHighlighted }) => <BubbleContentWrapper data-testid="polaris-lib-matrix.ui.bubble.default-bubble-component-wrapper" data-component-selector="bubble-content-wrapper-phaV">
		{!isDragging && <BubbleAurora isSelected={isSelected} isHighlighted={isHighlighted} borderColor={borderColor} color={color} isHovered={isHovered} />}
		<DefaultBubbleContainer color={color} borderColor={borderColor} isHovered={isHovered} isSelected={isSelected} isHighlighted={isHighlighted} isDragging={isDragging} />
	</BubbleContentWrapper>;
const getItemCountString = (items) => {
	if (items.length < 1e3) {
		return String(items.length);
	}
	return `${round(items.length / 1e3, 1)}K`;
};
export const DefaultBubbleClusterComponent = ({ items, color, borderColor, isHovered, isSelected, isDragging, isHighlighted }) => <BubbleContentWrapper data-testid="polaris-lib-matrix.ui.bubble.default-bubble-cluster-wrapper" data-component-selector="bubble-content-wrapper-phaV">
		{!isDragging && <BubbleAurora isSelected={isSelected} isHighlighted={isHighlighted} borderColor={borderColor} color={color} isHovered={isHovered} />}
		<DefaultBubbleContainer color={color} borderColor={borderColor} isHovered={isHovered} isSelected={isSelected} isHighlighted={isHighlighted} isDragging={isDragging}>
			<span>{getItemCountString(items)}</span>
		</DefaultBubbleContainer>
	</BubbleContentWrapper>;
export const SimpleDefaultBubbleClusterComponent = ({ items, isHovered, isSelected, isHighlighted }) => <BubbleContentWrapper data-testid="polaris-lib-matrix.ui.bubble.simple-default-bubble-cluster-wrapper" data-component-selector="bubble-content-wrapper-phaV">
		<BubbleAurora isSelected={isSelected} isHighlighted={isHighlighted} color={N300} borderColor={N300} isHovered={isHovered} />
		<DefaultBubbleContainer color={N300} borderColor={N300} isHovered={isHovered} isSelected={isSelected} isHighlighted={isHighlighted}>
			<span>{getItemCountString(items)}</span>
		</DefaultBubbleContainer>
	</BubbleContentWrapper>;
export const SimpleDefaultBubbleComponent = ({ isHovered, isSelected, isHighlighted }) => <BubbleContentWrapper data-testid="polaris-lib-matrix.ui.bubble.simple-default-bubble-wrapper" data-component-selector="bubble-content-wrapper-phaV">
		<BubbleAurora isSelected={isSelected} isHighlighted={isHighlighted} color={N300} borderColor={N300} isHovered={isHovered} />
		<DefaultBubbleContainer color={N300} borderColor={N300} isHovered={isHovered} isSelected={isSelected} isHighlighted={isHighlighted} />
	</BubbleContentWrapper>;
export const SimpleDefaultBubbleWrapperComponent = (props) => <>{props.children}</>;
export const DraggableBubble = ({ children, id, itemIds, onDragStart, onDragEnd, canDrop = true }) => {
	const { top, left } = useRelativeItemCenterPosition(id);
	const ref = useRef(null);
	useEffect(() => {
		if (!ref.current) return undefined;
		const cleanupDragAndDrop = combine(draggable({
			element: ref.current,
			getInitialData() {
				return {
					id,
					itemIds,
					type: DRAGGABLE_ITEM_TYPE,
					top,
					left,
					canDrop
				};
			},
			onDragStart() {
				if (itemIds.length > 1) {
					sendMatrixGroupDragOperationStarted();
				} else {
					sendMatrixSingularDragOperationStarted();
				}
				onDragStart();
			},
			onDrop() {
				if (itemIds.length > 1) {
					sendMatrixGroupDragOperationEnded();
				} else {
					sendMatrixSingularDragOperationEnded();
				}
				onDragEnd();
			}
		}));
		return () => {
			cleanupDragAndDrop?.();
		};
	}, [
		canDrop,
		id,
		itemIds,
		left,
		onDragEnd,
		onDragStart,
		top
	]);
	return <DraggableBubbleWrapper data-testid="polaris-lib-matrix.ui.bubble.draggable-bubble-wrapper" ref={ref}>
			{children}
		</DraggableBubbleWrapper>;
};
export const Bubble = memo(({ itemIds, isItemsDragDisabled }) => {
	const positioningId = itemIds[0];
	const [isDragging, setIsDragging] = useState(false);
	const toggleIsDragging = useCallback(() => setIsDragging((prev) => !prev), []);
	const BubbleRenderComponent = useItemRendererComponent();
	const BubbleClusterRenderComponent = useClusteredItemRendererComponent();
	const DraggableBubbleComponentProvided = useDraggableBubbleComponent();
	const InnerBubbleComponent = BubbleRenderComponent || SimpleDefaultBubbleComponent;
	const InnerBubbleClusterComponent = BubbleClusterRenderComponent || SimpleDefaultBubbleClusterComponent;
	const DraggableBubbleComponent = fg("jpd-aurora-roadmap-inline-edit") ? DraggableBubbleComponentProvided || DraggableBubble : DraggableBubble;
	const centerPosition = useRelativeItemCenterPosition(positioningId);
	const zPosition = useZAxisPositionForItems(itemIds);
	const highlightedItem = useHighlightedItem();
	const hoveredItem = useHoveredItems();
	const selectedItems = useSelectedItems();
	const { createAnalyticsEvent } = useAnalyticsEvents();
	const [, actions] = useMatrixActions();
	const { setHoveredItems, setSelectedItems } = actions;
	const isHovered = useMemo(() => intersection(itemIds, hoveredItem?.items || []).length > 0, [hoveredItem?.items, itemIds]);
	const isSelected = useMemo(() => intersection(selectedItems || [], itemIds || []).length > 0, [itemIds, selectedItems]);
	const isHighlighted = useMemo(() => !!highlightedItem && itemIds.includes(highlightedItem), [itemIds, highlightedItem]);
	const setSelectionFromClickEvent = useCallback((e) => {
		// do not propagate click event upwards to enable "outside-of-bubble-click" handling on the container
		e.stopPropagation();
		const analyticsAttributes = {
			bubbleType: "single",
			multiSelect: e.shiftKey || e.metaKey || e.ctrlKey
		};
		if (itemIds.length > 1) {
			sendMatrixGroupBubbleClicked();
			analyticsAttributes.bubbleType = "clustered";
		} else {
			sendMatrixSingletonBubbleClicked();
		}
		fireUIAnalytics(createAnalyticsEvent({
			action: "clicked",
			actionSubject: "icon"
		}), "ideaBubble", analyticsAttributes);
		const selected = selectedItems ?? [];
		if (selected.length === 0) {
			if (e.shiftKey) return;
			setSelectedItems([...itemIds]);
		} else if (e.metaKey || e.ctrlKey) {
			// in case current bubble is already selected, filter out current ones first
			const otherSelected = [...selected.filter((id) => !itemIds.includes(id))];
			setSelectedItems([...otherSelected, ...itemIds]);
		} else if (e.shiftKey) {
			setSelectedItems([...selected.filter((id) => !itemIds.includes(id))]);
		} else {
			setSelectedItems([...itemIds]);
		}
	}, [
		setSelectedItems,
		selectedItems,
		itemIds,
		createAnalyticsEvent
	]);
	const setHover = useCallback(() => {
		setHoveredItems({
			items: itemIds,
			area: "MATRIX"
		});
	}, [itemIds, setHoveredItems]);
	const clearHover = useCallback(() => {
		setHoveredItems(undefined);
	}, [setHoveredItems]);
	const renderInnerBubble = useCallback(
		() => {
			const InnerBubble = itemIds.length > 1 ? <InnerBubbleClusterComponent items={itemIds} isHovered={isHovered} isSelected={isSelected} isDragging={isDragging} isHighlighted={isHighlighted} /> : <InnerBubbleComponent id={positioningId} isHovered={isHovered} isSelected={isSelected} isDragging={isDragging} isHighlighted={isHighlighted} />;
			return isItemsDragDisabled ? InnerBubble : <DraggableBubbleComponent id={positioningId} itemIds={itemIds} onDragStart={toggleIsDragging} onDragEnd={toggleIsDragging}>
					{InnerBubble}
				</DraggableBubbleComponent>;
		},
		// eslint-disable-next-line react-hooks/react-compiler
		// eslint-disable-next-line react-hooks/exhaustive-deps
		[
			itemIds,
			positioningId,
			isSelected,
			isHovered,
			isDragging,
			isHighlighted
		]
	);
	return <div onMouseEnter={setHover} onMouseLeave={clearHover} onClick={setSelectionFromClickEvent}>
			<BubblePositioningComponent centerPosition={centerPosition} zPosition={zPosition} isHovered={isHovered} isDragging={isDragging} renderChildren={renderInnerBubble} />
		</div>;
});
const pulseAnimation = null;
const getConicGradient = (borderColor) => {
	const segmentAngle = 360 / borderColor.length;
	// 0deg is at top center position, we want it at left center
	const angleShift = -90;
	const gradientSegments = [];
	const overflowSegments = [];
	borderColor.forEach((segmentColor, index) => {
		const startAngle = index * segmentAngle + angleShift;
		const endAngle = (index + 1) * segmentAngle + angleShift;
		if (startAngle >= 0) {
			gradientSegments.push(`${segmentColor} ${startAngle}deg ${endAngle}deg`);
		} else {
			overflowSegments.push(`${segmentColor} ${360 + startAngle}deg ${360}deg`);
			gradientSegments.push(`${segmentColor} ${0}deg ${endAngle}deg`);
		}
	});
	return join([...gradientSegments, ...overflowSegments], ", ");
};
const getBorderColor = (isHovered, borderColor) => {
	if (isArray(borderColor) && borderColor.length > 1) {
		return "transparent";
	}
	let bc = borderColor;
	if (isArray(borderColor)) {
		[bc] = borderColor;
	} else if (isHovered) {
		bc = N500;
	}
	return bc;
};
const BORDER_THICKNESS = 3;
// eslint-disable-next-line @atlaskit/ui-styling-standard/no-styled -- To migrate as part of go/ui-styling-standard
const BubbleContentWrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[
		_,
		_2,
		_3,
		_4,
		_5,
		_6
	]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_2rko1rr0 _vchhusvi _kqswh2mm _bfhk1j28 _1bsb1osq _4t3i1osq", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	BubbleContentWrapper.displayName = "BubbleContentWrapper";
}
// eslint-disable-next-line @atlaskit/ui-styling-standard/no-styled -- To migrate as part of go/ui-styling-standard
const BubbleAurora = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { isHighlighted, isSelected, borderColor, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_2,
		_7,
		_8,
		_9,
		_5,
		_6,
		_10,
		_11,
		_12,
		_13,
		_14,
		_15,
		_16,
		_17,
		_18,
		_19,
		_20,
		_21
	]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_1cid1vn": ix(__cmplp.isHighlighted ? B700 : __cmplp.isSelected ? B500 : 'initial'),
		"--_4r9qlu": ix(getConicGradient(isArray(__cmplp.borderColor) && __cmplp.borderColor.length > 0 ? __cmplp.borderColor : [N500]), ")", "conic-gradient("),
		"--_s99dce": ix(!__cmplp.isSelected && "content-box, border-box")
	}} ref={__cmplr} className={ax([
		"_2rko1rr0 _vchhusvi _kqswstnw _1ltvidpf _154iidpf _1bsb1osq _4t3i1osq _bfhk10mk _1lrw174u _12vemgnk _ouxl1eba _qjkj9dsr _1y1mxe2f _15i11hgy _1canymdr _nqoi12c5",
		!__cmplp.isSelected && !__cmplp.isHighlighted ? "_1itk1wi1" : "_1itkglyw",
		__cmplp.className
	])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	BubbleAurora.displayName = "BubbleAurora";
}
// eslint-disable-next-line @atlaskit/ui-styling-standard/no-styled -- To migrate as part of go/ui-styling-standard
const DefaultBubbleContainer = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { isHighlighted, isSelected, isHovered, borderColor, isDragging, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_22,
		_23,
		_24,
		_5,
		_6,
		_7,
		_8,
		_9,
		_2,
		_25,
		_26,
		_27,
		_28,
		_29,
		_30,
		_12,
		_13,
		_14,
		_31,
		_32,
		_33,
		_34,
		_35,
		_36,
		_37,
		_38,
		_39
	]}</CS>
        <C {...__cmpldp} style={{
		...__cmpls,
		"--_1r7z21x": ix(!__cmplp.isHighlighted && !__cmplp.isSelected && `${BORDER_THICKNESS}px solid ${getBorderColor(__cmplp.isHovered, __cmplp.borderColor)}`),
		"--_ur3htu": ix(!__cmplp.isSelected && isArray(__cmplp.borderColor) && __cmplp.borderColor.length > 0 && `linear-gradient(${__cmplp.isHighlighted ? B700 : __cmplp.isSelected ? B500 : __cmplp.isHovered ? N500 : __cmplp.color}, ${__cmplp.isHighlighted ? B700 : __cmplp.isSelected ? B500 : __cmplp.isHovered ? N500 : __cmplp.color}), conic-gradient(${getConicGradient(__cmplp.borderColor)})`),
		"--_1mbry0z": ix(__cmplp.isHighlighted ? B700 : __cmplp.isSelected ? B500 : __cmplp.color),
		"--_1xpu8xd": ix(isArray(__cmplp.borderColor) && __cmplp.borderColor.length > 0 && 'content-box, border-box'),
		"--_sjyp98": ix(N0),
		"--_1i123mx": ix(N1000),
		"--_t0t6fo": ix(__cmplp.isHighlighted ? B700 : __cmplp.isSelected ? B500 : isArray(__cmplp.borderColor) && __cmplp.borderColor.length === 1 ? __cmplp.borderColor[0] : N500),
		"--_qk6zit": ix(!__cmplp.isSelected && isArray(__cmplp.borderColor) && __cmplp.borderColor.length === 1 && `linear-gradient(${__cmplp.borderColor[0]}, ${__cmplp.borderColor[0]}), conic-gradient(${getConicGradient(__cmplp.borderColor)})`)
	}} ref={__cmplr} className={ax([
		"_2rko1rr0 _11c8rymc _v5641bs3 _19itgqsr _1bsb1osq _4t3i1osq _kqswstnw _1ltvidpf _154iidpf _vchhusvi _1e0c1txw _4cvr1h6o _1bah1h6o _1itknqgl _bfhkwbqm _1lrwwvw6 _12vemgnk _ouxl1eba _qjkj9dsr _1y1m1pao _1yxgglyw _eyzjklnc _ejyi1nji _d0altlke",
		__cmplp.isDragging ? "_tzy4idpf" : "_tzy4kb7n",
		__cmplp.isHighlighted || __cmplp.isSelected || __cmplp.isHovered ? "_syazfl7h" : "_syazt8es",
		__cmplp.className
	])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	DefaultBubbleContainer.displayName = "DefaultBubbleContainer";
}
// eslint-disable-next-line @atlaskit/ui-styling-standard/no-styled -- To migrate as part of go/ui-styling-standard
const DraggableBubbleWrapper = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	return <CC>
        <CS>{[_40]}</CS>
        <C {...__cmplp} style={__cmpls} ref={__cmplr} className={ax(["_4t3i1kw7", __cmplp.className])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	DraggableBubbleWrapper.displayName = "DraggableBubbleWrapper";
}
// eslint-disable-next-line @atlaskit/ui-styling-standard/no-styled -- To migrate as part of go/ui-styling-standard
const BubblePositioningContainer = forwardRef(({ as: C = "div", style: __cmpls, ...__cmplp }, __cmplr) => {
	if (__cmplp.innerRef) {
		throw new Error("Please use 'ref' instead of 'innerRef'.");
	}
	const { isDragging, isHovered, ...__cmpldp } = __cmplp;
	return <CC>
        <CS>{[
		_,
		_7,
		_2,
		_41,
		_42,
		_43,
		_44,
		_45
	]}</CS>
        <C {...__cmpldp} style={__cmpls} ref={__cmplr} className={ax([
		"_2rko1rr0 _kqswstnw _vchhusvi _17wj1kw7",
		__cmplp.isDragging ? "_v56418uv" : "_v5641hpz",
		__cmplp.isHovered ? "_1pby11wp" : "_1pbycs5v",
		__cmplp.className
	])} />
      </CC>;
});
if (process.env.NODE_ENV !== "production") {
	BubblePositioningContainer.displayName = "BubblePositioningContainer";
}
