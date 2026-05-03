import { styled, keyframes } from '@compiled/react';

const fadeIn = keyframes({
  from: {
    opacity: 0,
  },
  to: {
    opacity: 1,
  },
});

const FadingDiv = styled.div`
  animation: ${fadeIn} 1s ease-in;
`;
