// TODO: EDITOR-6833 - Expected across this entire file, future violations are expected. Will try to remove them later after fully migration
/* eslint-disable @atlaskit/ui-styling-standard/no-nested-selectors, @atlaskit/ui-styling-standard/no-unsafe-selectors, @atlaskit/platform/expand-spacing-shorthand, @atlaskit/platform/expand-border-shorthand, @atlaskit/platform/expand-background-shorthand */
/**
 * @jsxRuntime classic
 * @jsx jsx
 * Compiled migration: platform_editor_core_static_css
 */
import React from 'react';

import { css, cssMap, jsx, keyframes } from '@compiled/react';

// eslint-disable-next-line import/order
import { getBrowserInfo } from '@atlaskit/editor-common/browser';

// eslint-disable-next-line @atlaskit/editor/enforce-todo-comment-format
// TODO: add back tableSharedStyle when migrate table styles
// import { richMediaClassName, tableSharedStyle } from '@atlaskit/editor-common/styles';
import { PanelSharedCssClassName } from '@atlaskit/editor-common/panel';
import {
	AnnotationSharedClassNames,
	richMediaClassName,
	expandClassNames,
	SmartCardSharedCssClassName,
	CodeBlockSharedCssClassName,
} from '@atlaskit/editor-common/styles';
import type {
	EditorAppearance,
	EditorContentMode,
	FeatureFlags,
} from '@atlaskit/editor-common/types';
import {
	akEditorFullPageDefaultFontSize,
	akEditorFullPageDenseFontSize,
	akEditorGutterPaddingDynamic,
	editorFontSize,
} from '@atlaskit/editor-shared-styles';
export type EditorContentContainerProps = {
	appearance?: EditorAppearance;
	children?: React.ReactNode;
	className?: string;
	contentMode?: EditorContentMode;
	featureFlags?: FeatureFlags;
	isScrollable?: boolean;
	/**
	 * When true, nodes maintain their standard width without negative margins
	 * For when the drag handle is visible and the editor has limited space.
	 */
	useStandardNodeWidth?: boolean;
	viewMode?: 'view' | 'edit';
};

/**
 * EditorContentStyles is a wrapper component that applies styles to its children
 * based on the provided feature flags, view mode, and other props.
 * It uses Emotion for styling and supports scrollable content.
 *
 * This will be used in near future to replace the current editor content styles from index.tsx
 */
export const EditorContentContainerCompiled: React.ForwardRefExoticComponent<
	EditorContentContainerProps & React.RefAttributes<HTMLDivElement>
> = React.forwardRef<HTMLDivElement, EditorContentContainerProps>((props, ref) => {
	const {
		className,
		children,
		viewMode,
		isScrollable,
		appearance,
		contentMode,
		useStandardNodeWidth,
	} = props;
	const { colorMode } = useThemeObserver();

	const isFullPage =
		appearance === 'full-page' ||
		appearance === 'full-width' ||
		((expValEqualsNoExposure('editor_tinymce_full_width_mode', 'isEnabled', true) ||
			expValEqualsNoExposure('confluence_max_width_content_appearance', 'isEnabled', true)) &&
			appearance === 'max');
	const isComment = appearance === 'comment';
	const isChromeless = appearance === 'chromeless';

	const baseFontSize = getBaseFontSize(appearance, contentMode);
	const isDense = !!baseFontSize && baseFontSize !== akEditorFullPageDefaultFontSize;

	const style = editorExperiment('platform_editor_preview_panel_responsiveness', true, {
		exposure: true,
	})
		? {
				'--ak-editor-base-font-size': `${editorFontSize({ theme: { baseFontSize } })}px`,
			}
		: {
				'--ak-editor-base-font-size': `${editorFontSize({ theme: { baseFontSize } })}px`,

				'--ak-editor--large-gutter-padding': `${akEditorGutterPaddingDynamic()}px`,
			};

	const browser = getBrowserInfo();

	return (
		<div
			// eslint-disable-next-line @atlaskit/ui-styling-standard/no-classname-prop -- Ignored via go/DSP-18766
			className={className}
			ref={ref}
			css={[
				editorContentStyles.baseStyles,
				editorContentStyles.maxModeReizeFixStyles,
				editorContentStyles.baseStylesMaxContainerWidthFixes,
				// eslint-disable-next-line @atlaskit/platform/no-preconditioning
				fg('platform_editor_controls_increase_full_page_gutter') &&
				editorExperiment('platform_editor_controls', 'variant1')
					? editorContentStyles.editorLargeGutterPuddingBaseStylesEditorControls
					: editorContentStyles.editorLargeGutterPuddingBaseStyles,
				editorExperiment('platform_editor_preview_panel_responsiveness', true, {
					exposure: true,
				}) && editorContentStyles.editorLargeGutterPuddingReducedBaseStyles,
				editorContentStyles.whitespaceStyles,
				editorContentStyles.indentationStyles,
				expValEquals('platform_editor_small_font_size', 'isEnabled', true) &&
					editorContentStyles.fontSizeStyles,
				editorContentStyles.shadowStyles,
				editorContentStyles.InlineNodeViewSharedStyles,
				editorContentStyles.hideSelectionStyles,
				editorContentStyles.hideCursorWhenHideSelectionStyles,
				editorContentStyles.selectedNodeStyles,
				editorContentStyles.cursorStyles,
				editorContentStyles.firstFloatingToolbarButtonStyles,
				editorContentStyles.placeholderTextStyles,
				editorContentStyles.placeholderStyles,
				editorExperiment('platform_editor_controls', 'variant1') &&
					editorContentStyles.placeholderOverflowStyles,
				editorExperiment('platform_editor_controls', 'variant1') &&
					fg('platform_editor_quick_insert_placeholder') &&
					editorContentStyles.placeholderWrapStyles,
				editorContentStyles.codeBlockStyles,
				contentMode === 'compact' &&
					(expValEquals('confluence_compact_text_format', 'isEnabled', true) ||
						// eslint-disable-next-line @atlaskit/platform/no-preconditioning
						(expValEquals('cc_editor_ai_content_mode', 'variant', 'test') &&
							fg('platform_editor_content_mode_button_mvp'))) &&
					editorContentStyles.codeBlockStylesWithEmUnits,
				!fg('platform_editor_typography_ugc') && editorContentStyles.editorUGCTokensDefault,
				fg('platform_editor_typography_ugc') && editorContentStyles.editorUGCTokensRefreshed,
				expValEquals('platform_editor_small_font_size', 'isEnabled', true) &&
					editorContentStyles.editorUGCSmallText,
				editorContentStyles.blocktypeStyles,
				editorExperiment('platform_editor_block_menu', true, { exposure: true }) &&
					editorContentStyles.blockquoteSelectedNodeStyles,
				editorExperiment('platform_editor_block_menu', true, { exposure: true }) &&
					editorContentStyles.listSelectedNodeStyles,
				editorExperiment('platform_editor_block_menu', true, { exposure: true }) &&
					editorContentStyles.textSelectedNodeStyles,
				fg('platform_editor_typography_ugc')
					? editorContentStyles.blocktypeStyles_fg_platform_editor_typography_ugc
					: editorContentStyles.blocktypeStyles_without_fg_platform_editor_typography_ugc,
				fg('platform_editor_nested_dnd_styles_changes') &&
					editorContentStyles.blocktypeStyles_fg_platform_editor_nested_dnd_styles_changes,
				editorContentStyles.codeMarkStyles,
				expValEquals('platform_editor_a11y_scrollable_region', 'isEnabled', true) &&
					editorContentStyles.codeMarkStylesA11yFix,
				editorContentStyles.textColorStyles,
				editorContentStyles.backgroundColorStyles,
				editorContentStyles.textHighlightPaddingStyles,
				editorContentStyles.listsStyles,
				expValEqualsNoExposure('platform_editor_flexible_list_schema', 'isEnabled', true) &&
					editorContentStyles.listItemHiddenMarkerStyles,
				editorContentStyles.diffListStyles,
				// Condense vertical spacing between list items when content mode dense is active
				contentMode === 'compact' &&
					(expValEquals('confluence_compact_text_format', 'isEnabled', true) ||
						// eslint-disable-next-line @atlaskit/platform/no-preconditioning
						(expValEquals('cc_editor_ai_content_mode', 'variant', 'test') &&
							fg('platform_editor_content_mode_button_mvp'))) &&
					isDense &&
					editorContentStyles.listsDenseStyles,
				expValEquals('cc_editor_ttvc_release_bundle_one', 'listLayoutShiftFix', true) &&
					isFullPage &&
					editorContentStyles.listsStylesMarginLayoutShiftFix,
				editorContentStyles.ruleStyles,
				editorContentStyles.smartCardDiffStyles,
				expValEquals('platform_editor_enghealth_a11y_jan_fixes', 'isEnabled', true)
					? editorContentStyles.showDiffDeletedNodeStylesNew
					: editorContentStyles.showDiffDeletedNodeStyles,
				editorContentStyles.mediaStyles,
				contentMode === 'compact' &&
					(expValEquals('confluence_compact_text_format', 'isEnabled', true) ||
						// eslint-disable-next-line @atlaskit/platform/no-preconditioning
						(expValEquals('cc_editor_ai_content_mode', 'variant', 'test') &&
							fg('platform_editor_content_mode_button_mvp'))) &&
					editorContentStyles.mediaCaptionStyles,
				// merge firstWrappedMediaStyles with mediaStyles when clean up platform_editor_fix_media_in_renderer
				fg('platform_editor_fix_media_in_renderer') && editorContentStyles.firstWrappedMediaStyles,
				editorContentStyles.telepointerStyle,
				/* This needs to be after telepointer styles as some overlapping rules have equal specificity, and so the order is significant */
				editorContentStyles.telepointerColorAndCommonStyle,
				editorContentStyles.gapCursorStyles,
				editorExperiment('platform_synced_block', true) &&
					editorContentStyles.gapCursorStylesVisibilityFix,
				editorContentStyles.panelStyles,
				editorContentStyles.nestedPanelBorderStylesMixin,
				fg('platform_editor_nested_dnd_styles_changes') &&
					editorContentStyles.panelStylesMixin_fg_platform_editor_nested_dnd_styles_changes,
				editorContentStyles.panelStylesMixin,
				editorContentStyles.mentionsStyles,
				editorContentStyles.tasksAndDecisionsStyles,
				// condense vertical spacing between tasks/decisions items when content mode dense is active
				// eslint-disable-next-line @atlaskit/editor/enforce-todo-comment-format
				// TODO: uncomment and remove dynamic styles from getDenseTasksAndDecisionsStyles
				// migrate this with packages/editor/editor-core/src/ui/EditorContentContainer/styles/tasksAndDecisionsStyles.ts
				// reference: https://atlassian.design/components/eslint-plugin-ui-styling-standard/no-dynamic-styles/usage
				// contentMode === 'compact' &&
				// 	(expValEquals('confluence_compact_text_format', 'isEnabled', true) ||
				// 		// eslint-disable-next-line @atlaskit/platform/no-preconditioning
				// 		(expValEquals('cc_editor_ai_content_mode', 'variant', 'test') &&
				// 			fg('platform_editor_content_mode_button_mvp'))) &&
				// 	getDenseTasksAndDecisionsStyles(baseFontSize),
				editorContentStyles.gridStyles,
				editorContentStyles.blockMarksStyles,
				editorContentStyles.dateStyles,
				// eslint-disable-next-line @atlaskit/editor/enforce-todo-comment-format
				// TODO: uncomment and remove dynamic styles from getExtensionStyles
				// migrate this with packages/editor/editor-core/src/ui/EditorContentContainer/styles/extensionStyles.ts
				// suggest creating a new cssMap for the variant use case from the guide below
				// reference: https://atlassian.design/components/eslint-plugin-ui-styling-standard/no-dynamic-styles/usage
				// getExtensionStyles(contentMode),
				editorContentStyles.extensionDiffStyles,
				editorContentStyles.expandStylesBase,
				// Apply expand delta styles conditionally based on useStandardNodeWidth (negative margins or not)
				!useStandardNodeWidth && editorContentStyles.expandStyles,
				contentMode === 'compact' &&
				(expValEquals('confluence_compact_text_format', 'isEnabled', true) ||
					// eslint-disable-next-line @atlaskit/platform/no-preconditioning
					(expValEquals('cc_editor_ai_content_mode', 'variant', 'test') &&
						fg('platform_editor_content_mode_button_mvp'))) &&
				// eslint-disable-next-line @atlaskit/editor/enforce-todo-comment-format
				// TODO: uncomment and remove dynamic styles from getDenseExpandTitleStyles
				// migrate this with packages/editor/editor-core/src/ui/EditorContentContainer/styles/expandStyles.ts
				// getDenseExpandTitleStyles(baseFontSize),
				fg('platform_editor_nested_dnd_styles_changes')
					? editorContentStyles.expandStylesMixin_fg_platform_editor_nested_dnd_styles_changes
					: editorContentStyles.expandStylesMixin_without_fg_platform_editor_nested_dnd_styles_changes,
				editorContentStyles.expandStylesMixin_fg_platform_visual_refresh_icons,
				isChromeless &&
					expValEquals('platform_editor_chromeless_expand_fix', 'isEnabled', true) &&
					editorContentStyles.expandStylesMixin_experiment_platform_editor_chromeless_expand_fix,
				expValEquals('platform_editor_find_and_replace_improvements', 'isEnabled', true)
					? editorContentStyles.findReplaceStylesNewWithA11Y
					: editorContentStyles.findReplaceStyles,
				expValEquals('platform_editor_find_and_replace_improvements', 'isEnabled', true) &&
					editorContentStyles.findReplaceStylesNewWithCodeblockColorContrastFix,
				!expValEquals('platform_editor_find_and_replace_improvements', 'isEnabled', true) &&
					editorContentStyles.findReplaceStylesWithCodeblockColorContrastFix,
				editorContentStyles.textHighlightStyle,
				editorContentStyles.decisionStyles,
				expValEqualsNoExposure('platform_editor_blocktaskitem_node_tenantid', 'isEnabled', true)
					? editorContentStyles.taskItemStylesWithBlockTaskItem
					: editorContentStyles.taskItemStyles,
				editorContentStyles.taskItemCheckboxStyles,
				editorContentStyles.decisionIconWithVisualRefresh,
				editorContentStyles.statusStyles,
				fg('platform-dst-lozenge-tag-badge-visual-uplifts')
					? editorContentStyles.statusStylesTeam26
					: fg('platform-component-visual-refresh')
						? expValEqualsNoExposure(
								'platform_editor_find_and_replace_improvements',
								'isEnabled',
								true,
							)
							? editorContentStyles.statusStylesMixin_fg_platform_component_visual_refresh_with_search_match
							: editorContentStyles.statusStylesMixin_fg_platform_component_visual_refresh
						: expValEqualsNoExposure(
									'platform_editor_find_and_replace_improvements',
									'isEnabled',
									true,
							  )
							? editorContentStyles.statusStylesMixin_without_fg_platform_component_visual_refresh_with_search_match
							: editorContentStyles.statusStylesMixin_without_fg_platform_component_visual_refresh,
				editorContentStyles.annotationStyles,
				expValEqualsNoExposure('platform_editor_find_and_replace_improvements', 'isEnabled', true)
					? editorExperiment('platform_editor_block_menu', true)
						? editorContentStyles.smartCardStylesWithSearchMatchAndBlockMenuDangerStyles
						: editorContentStyles.smartCardStylesWithSearchMatch
					: editorContentStyles.smartCardStyles,
				editorExperiment('platform_editor_preview_panel_responsiveness', true) &&
					editorContentStyles.smartCardStylesWithSearchMatchAndPreviewPanelResponsiveness,
				(expValEqualsNoExposure('platform_editor_controls', 'cohort', 'variant1') ||
					editorExperiment('platform_editor_preview_panel_linking_exp', true)) &&
					editorContentStyles.editorControlsSmartCardStyles,
				editorContentStyles.embedCardStyles,
				editorContentStyles.unsupportedStyles,
				editorContentStyles.resizerStyles,
				editorContentStyles.layoutBaseStyles,
				expValEquals('platform_editor_table_excerpts_fix', 'isEnabled', true) &&
					editorContentStyles.layoutBaseStylesWithTableExcerptsFix,
				// merge alignMultipleWrappedImageInLayoutStyles with layoutBaseStyles when clean up platform_editor_fix_media_in_renderer
				fg('platform_editor_fix_media_in_renderer') &&
					editorContentStyles.alignMultipleWrappedImageInLayoutStyles,
				editorExperiment('platform_synced_block', true) && editorContentStyles.syncBlockStylesBase,
				editorExperiment('platform_synced_block', true) &&
					// Apply sync block delta styles conditionally based on useStandardNodeWidth (negative margins or not)
					!useStandardNodeWidth &&
					editorContentStyles.syncBlockStyles,
				editorExperiment('platform_synced_block', true) &&
					editorContentStyles.syncBlockOverflowStyles,
				editorExperiment('platform_synced_block', true) &&
					editorContentStyles.syncBlockFirstNodeStyles,
				editorExperiment('advanced_layouts', true) && editorContentStyles.layoutBaseStylesAdvanced,
				editorExperiment('advanced_layouts', true)
					? editorContentStyles.layoutSectionStylesAdvanced
					: editorContentStyles.layoutSectionStylesNotAdvanced,
				editorExperiment('advanced_layouts', true) &&
					editorExperiment('platform_editor_layout_column_resize_handle', true) &&
					editorContentStyles.layoutColumnDividerStyles,
				editorExperiment('advanced_layouts', true) &&
					editorExperiment('platform_editor_layout_column_resize_handle', true) &&
					fg('platform_editor_nested_dnd_styles_changes') &&
					editorContentStyles.layoutColumnDividerStylesNestedDnD,
				editorExperiment('advanced_layouts', true)
					? editorContentStyles.layoutColumnStylesAdvanced
					: editorContentStyles.layoutColumnStylesNotAdvanced,
				editorExperiment('advanced_layouts', true) &&
					editorExperiment('platform_editor_layout_column_resize_handle', true) &&
					editorContentStyles.layoutColumnResizeStyles,
				editorExperiment('advanced_layouts', true)
					? editorContentStyles.layoutSelectedStylesAdvanced
					: editorContentStyles.layoutSelectedStylesNotAdvanced,
				editorExperiment('platform_synced_block', true) &&
					editorContentStyles.layoutSelectedStylesAdvancedFix,
				editorExperiment('advanced_layouts', true) &&
					editorContentStyles.layoutColumnResponsiveStyles,
				editorExperiment('advanced_layouts', true) &&
					editorContentStyles.layoutResponsiveBaseStyles,
				editorExperiment('platform_synced_block', true) &&
					fg('platform_editor_nested_dnd_styles_changes') &&
					editorContentStyles.layoutBaseStylesFixesUnderNestedDnDFGExcludingBodiedSync,
				!editorExperiment('platform_synced_block', true) &&
					fg('platform_editor_nested_dnd_styles_changes') &&
					editorContentStyles.layoutBaseStylesFixesUnderNestedDnDFG,
				fg('platform_editor_nested_dnd_styles_changes')
					? editorContentStyles.layoutColumnMartinTopFixesNew
					: editorContentStyles.layoutColumnMartinTopFixesOld,
				editorContentStyles.smartLinksInLivePagesStyles,
				editorContentStyles.linkingVisualRefreshV1Styles,
				editorContentStyles.dateVanillaStyles,
				fg('platform_editor_typography_ugc')
					? contentMode === 'compact' &&
						(expValEquals('confluence_compact_text_format', 'isEnabled', true) ||
							// eslint-disable-next-line @atlaskit/platform/no-preconditioning
							(expValEquals('cc_editor_ai_content_mode', 'variant', 'test') &&
								fg('platform_editor_content_mode_button_mvp')))
						? editorContentStyles.paragraphStylesWithScaledMargin
						: editorContentStyles.paragraphStylesUGCRefreshed
					: contentMode === 'compact' &&
						  (expValEquals('confluence_compact_text_format', 'isEnabled', true) ||
								// eslint-disable-next-line @atlaskit/platform/no-preconditioning
								(expValEquals('cc_editor_ai_content_mode', 'variant', 'test') &&
									fg('platform_editor_content_mode_button_mvp')))
						? editorContentStyles.paragraphStylesOldWithScaledMargin
						: editorContentStyles.paragraphStylesOld,
				editorContentStyles.linkStyles,
				browser.safari && editorContentStyles.listsStylesSafariFix,
				editorExperiment('platform_synced_block', true) &&
					editorContentStyles.pragmaticResizerStylesSyncedBlock,
				expValEqualsNoExposure('platform_editor_breakout_resizing', 'isEnabled', true)
					? editorContentStyles.pragmaticResizerStyles
					: undefined,
				expValEqualsNoExposure('platform_editor_breakout_resizing', 'isEnabled', true)
					? editorExperiment('platform_synced_block', true)
						? editorContentStyles.pragmaticResizerStylesCodeBlockSyncedBlockPatch
						: editorContentStyles.pragmaticResizerStylesCodeBlockLegacy
					: undefined,
				editorExperiment('advanced_layouts', true) &&
					expValEqualsNoExposure('platform_editor_breakout_resizing', 'isEnabled', true) &&
					editorContentStyles.pragmaticStylesLayoutFirstNodeResizeHandleFix,
				expValEqualsNoExposure('platform_editor_breakout_resizing', 'isEnabled', true) &&
					editorContentStyles.pragmaticResizerStylesForTooltip,
				editorExperiment('platform_editor_preview_panel_responsiveness', true) &&
					(editorExperiment('advanced_layouts', true) ||
						expValEqualsNoExposure('platform_editor_breakout_resizing', 'isEnabled', true)) &&
					editorContentStyles.pragmaticResizerStylesWithReducedEditorGutter,
				editorContentStyles.aiPanelBaseStyles,
				isFirefox && editorContentStyles.aiPanelBaseFirefoxStyles,
				colorMode === 'dark' && editorContentStyles.aiPanelDarkStyles,
				colorMode === 'dark' && isFirefox && editorContentStyles.aiPanelDarkFirefoxStyles,
				viewMode === 'view' && editorContentStyles.layoutStylesForView,
				viewMode === 'view' &&
					editorExperiment('advanced_layouts', true) &&
					editorContentStyles.layoutSelectedStylesForViewAdvanced,
				viewMode === 'view' &&
					editorExperiment('advanced_layouts', false) &&
					editorContentStyles.layoutSelectedStylesForViewNotAdvanced,
				viewMode === 'view' &&
					editorExperiment('advanced_layouts', true) &&
					editorContentStyles.layoutResponsiveStylesForView,
				isComment && editorContentStyles.commentEditorStyles,
				isComment && editorContentStyles.tableCommentEditorStyles,
				isFullPage && editorContentStyles.fullPageEditorStyles,
				isFullPage && editorContentStyles.scrollbarStyles,
				fg('platform_editor_nested_dnd_styles_changes')
					? editorContentStyles.firstCodeBlockWithNoMargin
					: editorContentStyles.firstCodeBlockWithNoMarginOld,
				editorContentStyles.firstBlockNodeStyles,
				editorContentStyles.mentionNodeStyles,
				expValEqualsNoExposure('platform_editor_find_and_replace_improvements', 'isEnabled', true)
					? editorContentStyles.mentionsSelectionStylesWithSearchMatch
					: editorContentStyles.mentionsSelectionStyles,
				expValEquals('platform_editor_lovability_emoji_scaling', 'isEnabled', true)
					? editorContentStyles.scaledEmojiStyles
					: editorContentStyles.emojiStyles,
				// eslint-disable-next-line @atlaskit/editor/enforce-todo-comment-format
				// TODO: uncomment and remove dynamic styles from getScaledDenseEmojiStyles and getDenseEmojiStyles
				// when migrate with packages/editor/editor-core/src/ui/EditorContentContainer/styles/emoji.ts
				// Dense emoji scaling based on base font size
				// contentMode === 'compact' &&
				// (expValEquals('confluence_compact_text_format', 'isEnabled', true) ||
				// 	// eslint-disable-next-line @atlaskit/platform/no-preconditioning
				// 	(expValEquals('cc_editor_ai_content_mode', 'variant', 'test') &&
				// 		fg('platform_editor_content_mode_button_mvp')))
				// 	? expValEquals('platform_editor_lovability_emoji_scaling', 'isEnabled', true)
				// 		? // eslint-disable-next-line @atlaskit/ui-styling-standard/no-imported-style-values
				// 			getScaledDenseEmojiStyles(baseFontSize)
				// 		: // eslint-disable-next-line @atlaskit/ui-styling-standard/no-imported-style-values
				// 			getDenseEmojiStyles(baseFontSize)
				// 	: undefined,
				editorContentStyles.panelViewStyles,
				editorContentStyles.mediaGroupStyles,
				editorContentStyles.mediaAlignmentStyles,
				expValEquals('platform_editor_small_font_size', 'isEnabled', true)
					? editorContentStyles.tableLayoutFixesWithFontSize
					: editorContentStyles.tableLayoutFixes,
				editorContentStyles.tableContainerStyles,
				// eslint-disable-next-line @atlaskit/editor/enforce-todo-comment-format
				// TODO: it was from "import { tableSharedStyle } from '@atlaskit/editor-common/styles';"
				// tableSharedStyle(),
				editorContentStyles.tableEmptyRowStyles,
				expValEquals('platform_editor_table_fit_to_content_auto_convert', 'isEnabled', true) &&
					editorContentStyles.tableContentModeStyles,
				editorContentStyles.hyperLinkFloatingToolbarStyles,
				editorContentStyles.selectionToolbarAnimationStyles,
				editorExperiment('platform_editor_block_menu', true) && [
					editorContentStyles.blockquoteDangerStyles,
					editorContentStyles.textDangerStyles,
					editorContentStyles.listDangerStyles,
					editorContentStyles.dangerDateStyles,
					editorContentStyles.emojiDangerStyles,
					editorContentStyles.mentionDangerStyles,
					editorContentStyles.decisionDangerStyles,
					editorContentStyles.statusDangerStyles,
					editorContentStyles.dangerRuleStyles,
					editorContentStyles.mediaDangerStyles,
					editorContentStyles.nestedPanelDangerStyles,
				],
			]}
			data-editor-scroll-container={isScrollable ? 'true' : undefined}
			data-testid="editor-content-container"
			// eslint-disable-next-line @atlaskit/ui-styling-standard/enforce-style-prop
			style={style as React.CSSProperties}
			tabIndex={isScrollable ? 0 : undefined}
			role={isScrollable ? 'region' : undefined}
		>
			{children}
		</div>
	);
});