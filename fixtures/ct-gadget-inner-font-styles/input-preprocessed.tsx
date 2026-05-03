/** @jsx jsx */
import React from 'react';
import { css, jsx } from '@compiled/react';
export const Gadget = () => {
  const fullWidthStyles = css({
    width: '100%'
  });
  return <div css={[fullWidthStyles, fontStyles]} />;
};
const fontStyles = css({
  fontFamily: "var(--ds-font-family-body, \"Atlassian Sans\", ui-sans-serif, -apple-system, BlinkMacSystemFont, \"Segoe UI\", Ubuntu, \"Helvetica Neue\", sans-serif)"
});