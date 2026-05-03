import { styled, css } from '@compiled/react';
const READ_VIEW_CONTAINER_SELECTOR = '[data-component-selector=jira-issue-field-inline-edit-read-view-container]';
const ORIGINAL_READ_VIEW_SELECTOR = '[data-component-selector=jira-issue-field-original-estimate-read-view-container]';
const Wrapper = styled.div({
  [READ_VIEW_CONTAINER_SELECTOR]: {
    whiteSpace: 'nowrap',
    marginLeft: "var(--ds-space-negative-025, -2px)",
    marginBottom: 0,
    marginTop: `calc(${"var(--ds-space-negative-100, -8px)"} * 0.125)`,
    marginRight: 0
  },
  height: "var(--ds-space-200, 16px)"
}, ({
  disableClick
}) => disableClick && css({
  pointerEvents: 'none'
}), {
  [`&:not(:has(${ORIGINAL_READ_VIEW_SELECTOR}, ${READ_VIEW_CONTAINER_SELECTOR}))`]: {
    zIndex: "var(--ds-surface, #FFFFFF)"
  }
}, ({
  isIncrementPlanningBoard
}) => isIncrementPlanningBoard && css({
  [`&:not(:has(${ORIGINAL_READ_VIEW_SELECTOR}, ${READ_VIEW_CONTAINER_SELECTOR}))`]: {
    marginRight: `calc(${"var(--ds-space-600, 48px)"} + ${"var(--ds-space-100, 8px)"})`
  }
}));
export const Component = ({
  disableClick,
  isIncrementPlanningBoard
}) => <Wrapper disableClick={disableClick} isIncrementPlanningBoard={isIncrementPlanningBoard} />;