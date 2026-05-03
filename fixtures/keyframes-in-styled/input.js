import { styled, keyframes } from '@compiled/react';

const fadeIn = keyframes({
  from: { opacity: 0 },
  to: { opacity: 1 },
});

const FadeInDiv = styled.div({
  animationName: fadeIn,
  animationDuration: '1s',
});
