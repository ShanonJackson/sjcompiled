import React from 'react';
import { styled } from '@compiled/react';
const extraTopOffset = -1;
const StickyWrapper = styled.div({
  '@supports (position: sticky) or (position: -webkit-sticky)': {
    position: 'sticky',
    background: props => props.isEmbedMode ? "var(--ds-surface-overlay, #FFFFFF)" : "var(--ds-surface, #FFFFFF)",
    zIndex: ({
      zIndex
    }) => zIndex,
    marginLeft: "var(--ds-space-negative-100, -8px)",
    top: props => `${props.topOffset + extraTopOffset}px`,
    boxShadow: props => props.showKeyline ? `0 ${"var(--ds-space-025, 2px)"} ${"var(--ds-border, #0B120E24)"}` : undefined,
    paddingBottom: ({
      showKeyline,
      applyVisualRefreshChanges
    }) => showKeyline && applyVisualRefreshChanges ? "var(--ds-space-100, 8px)" : undefined,
    paddingTop: ({
      showKeyline,
      applyVisualRefreshChanges,
      showPaddingTopOnlyOnKeyline
    }) => {
      if (showPaddingTopOnlyOnKeyline) {
        return showKeyline && applyVisualRefreshChanges ? "var(--ds-space-100, 8px)" : `${-extraTopOffset}px`;
      }
      return applyVisualRefreshChanges ? "var(--ds-space-100, 8px)" : `${-extraTopOffset}px`;
    },
    paddingRight: ({
      isTabLayout
    }) => isTabLayout ? "var(--ds-space-300, 24px)" : undefined,
    paddingLeft: ({
      isTabLayout,
      isSidebarPanelOpened
    }) =>
    // eslint-disable-next-line no-nested-ternary
    isSidebarPanelOpened ? 'unset' : isTabLayout ? "var(--ds-space-300, 24px)" : "var(--ds-space-100, 8px)"
  }
}, ({
  flexStyle,
  isWideLayout
}) => flexStyle && {
  display: 'flex',
  justifyContent: 'space-between',
  maxWidth: isWideLayout ? '1920px' : 'inherit',
  width: isWideLayout ? '100%' : 'inherit',
  justifySelf: isWideLayout ? 'center' : 'inherit'
});
const Fixture = () => <StickyWrapper topOffset={0} showKeyline isEmbedMode zIndex={10} applyVisualRefreshChanges flexStyle isWideLayout isTabLayout showPaddingTopOnlyOnKeyline isSidebarPanelOpened={false}>
    content
  </StickyWrapper>;
export default Fixture;