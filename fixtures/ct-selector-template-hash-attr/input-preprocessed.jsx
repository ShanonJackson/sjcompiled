import { styled } from '@compiled/react';
const READ_VIEW_CONTAINER_SELECTOR = '[data-component-selector=jira-issue-field-inline-edit-read-view-container]';
const Wrapper = styled.div({
  [`&:not(:has(${READ_VIEW_CONTAINER_SELECTOR}, ${READ_VIEW_CONTAINER_SELECTOR}))`]: {
    marginRight: `calc(${"var(--ds-space-600, 48px)"} + ${"var(--ds-space-100, 8px)"})`
  }
});
export const Component = () => <Wrapper />;