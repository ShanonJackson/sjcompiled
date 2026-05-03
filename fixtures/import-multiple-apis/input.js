import { styled, css, keyframes, ClassNames, cssMap } from '@compiled/react';

const fadeIn = keyframes({
  from: { opacity: 0 },
  to: { opacity: 1 },
});

const StyledDiv = styled.div({
  color: 'blue',
});

const MyComponent = () => (
  <div css={{ fontSize: '14px' }}>Hello</div>
);
