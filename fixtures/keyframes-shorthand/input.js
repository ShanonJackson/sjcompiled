import { styled, keyframes } from '@compiled/react';

const slideIn = keyframes({
  from: { transform: 'translateX(-100%)' },
  to: { transform: 'translateX(0)' },
});

const Slider = styled.div({
  animation: `${slideIn} 0.3s ease-in-out`,
});
