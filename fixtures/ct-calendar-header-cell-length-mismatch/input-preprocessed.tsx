/** @jsx jsx */
import { css, jsx, styled } from '@compiled/react';
const weekViewStyles = css({
  '.fc-theme-standard th .fc-col-header-cell-cushion>div': {
    '> div': {
      font: "var(--ds-font-heading-xxsmall, normal 653 12px/16px \"Atlassian Sans\", ui-sans-serif, -apple-system, BlinkMacSystemFont, \"Segoe UI\", Ubuntu, \"Helvetica Neue\", sans-serif)",
      color: "var(--ds-text-subtle, #505258)",
      fontWeight: "var(--ds-font-weight-semibold, 600)",
      width: 'auto',
      height: 'auto',
      paddingLeft: "var(--ds-space-050, 4px)"
    },
    '> span': {
      font: "var(--ds-font-heading-xxsmall, normal 653 12px/16px \"Atlassian Sans\", ui-sans-serif, -apple-system, BlinkMacSystemFont, \"Segoe UI\", Ubuntu, \"Helvetica Neue\", sans-serif)",
      color: "var(--ds-text-subtlest, #6B6E76)",
      fontWeight: "var(--ds-font-weight-semibold, 600)"
    },
    '> div > h4': {
      font: "var(--ds-font-heading-xxsmall, normal 653 12px/16px \"Atlassian Sans\", ui-sans-serif, -apple-system, BlinkMacSystemFont, \"Segoe UI\", Ubuntu, \"Helvetica Neue\", sans-serif)",
      color: "var(--ds-text-subtlest, #6B6E76)",
      fontWeight: "var(--ds-font-weight-semibold, 600)"
    }
  },
  '.fc-theme-standard th .fc-day-today .fc-col-header-cell-cushion>div': {
    '> div': {
      color: "var(--ds-text-brand, #1868DB)",
      background: 'none'
    },
    '> span': {
      color: "var(--ds-text-brand, #1868DB)",
      background: 'none'
    },
    '> div > h4': {
      color: "var(--ds-text-brand, #1868DB)",
      background: 'none'
    }
  }
});
const CalendarHeader = styled.div<{
  weekViewEnabled: boolean;
  isVisualRefresh?: boolean;
  hasIssuesPages?: boolean;
}>(({
  weekViewEnabled
}) => weekViewEnabled && weekViewStyles, {
  borderRadius: ({
    hasIssuesPages,
    isVisualRefresh
  }) =>
  // match the original CalendarRenderer border radius branching
  hasIssuesPages ? isVisualRefresh ? `${"var(--ds-space-075, 6px)"} ${"var(--ds-space-075, 6px)"} 0px 0px` : '2px 2px 0px 0px' : isVisualRefresh ? "var(--ds-space-075, 6px)" : "var(--ds-radius-xsmall, 2px)",
  '.fc-theme-standard th': {
    '.fc-scrollgrid-sync-inner': {
      display: 'flex',
      flexDirection: 'row',
      justifyContent: 'center',
      alignItems: 'center',
      height: ({
        isVisualRefresh
      }) => isVisualRefresh ? '40px' : '24px'
    },
    '.fc-col-header-cell-cushion': {
      paddingBottom: 0,
      ' > div': {
        color: "var(--ds-text-subtle, #505258)",
        font: "var(--ds-font-heading-xxsmall, normal 653 12px/16px \"Atlassian Sans\", ui-sans-serif, -apple-system, BlinkMacSystemFont, \"Segoe UI\", Ubuntu, \"Helvetica Neue\", sans-serif)",
        fontWeight: "var(--ds-font-weight-bold, 653)",
        display: ({
          weekViewEnabled
        }) => weekViewEnabled ? 'flex' : 'block',
        flexDirection: ({
          weekViewEnabled
        }) => weekViewEnabled && 'row',
        textTransform: 'capitalize'
      },
      ' > span': {
        color: "var(--ds-text-subtle, #505258)",
        font: "var(--ds-font-heading-xxsmall, normal 653 12px/16px \"Atlassian Sans\", ui-sans-serif, -apple-system, BlinkMacSystemFont, \"Segoe UI\", Ubuntu, \"Helvetica Neue\", sans-serif)",
        fontWeight: "var(--ds-font-weight-bold, 653)",
        display: ({
          weekViewEnabled
        }) => weekViewEnabled ? 'flex' : 'block',
        flexDirection: ({
          weekViewEnabled
        }) => weekViewEnabled && 'row',
        textTransform: 'capitalize'
      }
    }
  }
});
const CalendarHeaderFixture = ({
  weekViewEnabled = true
}) => <CalendarHeader weekViewEnabled={weekViewEnabled} isVisualRefresh>
		header
	</CalendarHeader>;
export default CalendarHeaderFixture;