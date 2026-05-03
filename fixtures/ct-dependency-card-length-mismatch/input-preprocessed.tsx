/** @jsx jsx */
import { css, jsx } from '@compiled/react';
const containerStylesOld = css({
  display: 'flex',
  flexDirection: 'column',
  position: 'relative',
  paddingTop: "var(--ds-space-100, 8px)",
  paddingRight: "var(--ds-space-200, 16px)",
  paddingBottom: "var(--ds-space-100, 8px)",
  paddingLeft: "var(--ds-space-200, 16px)",
  backgroundColor: "var(--ds-surface-raised, #FFFFFF)",
  boxShadow: "var(--ds-shadow-raised, 0px 1px 1px #1E1F2140, 0px 0px 1px #1E1F214f)"
});
const titleContainerStyles = css({
  display: 'flex',
  position: 'relative',
  flexDirection: 'row',
  alignItems: 'center',
  overflow: 'hidden'
});
const childContainerStyles = css({
  position: 'relative',
  paddingLeft: "var(--ds-space-300, 24px)",
  marginTop: "var(--ds-space-050, 4px)"
});
const DependencyCardFixture = ({
  withParent = true
}) => {
  const content = <div css={containerStylesOld}>content</div>;
  return withParent ? <>
      <div css={titleContainerStyles}>title</div>
      <div css={childContainerStyles}>{content}</div>
    </> : content;
};
export default DependencyCardFixture;