import { styled } from '@compiled/react';
const STATUS = {
  TODO: 'TODO',
  IN_PROGRESS: 'IN_PROGRESS',
  DONE: 'DONE',
  UNKNOWN: 'UNKNOWN'
} as const;
const STATUS_COLOR_MAP = {
  [STATUS.TODO]: "var(--ds-icon-accent-gray, #7D818A)",
  [STATUS.IN_PROGRESS]: "var(--ds-icon-accent-blue, #357DE8)",
  [STATUS.DONE]: "var(--ds-icon-accent-green, #22A06B)",
  [STATUS.UNKNOWN]: 'unset'
};
export const StatusBar = styled.div<{
  status: keyof typeof STATUS;
}>({
  backgroundColor: ({
    status
  }) => STATUS_COLOR_MAP[status],
  top: 0,
  left: 0,
  right: 0,
  height: "var(--ds-space-050, 4px)",
  borderRadius: `${"var(--ds-radius-medium, 6px)"} ${"var(--ds-radius-medium, 6px)"} 0 0`,
  position: 'absolute'
});