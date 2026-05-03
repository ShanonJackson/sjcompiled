import { keyframes, styled } from '@compiled/react';

const fadeIn = keyframes`
  from { opacity: 0; }
  to { opacity: 1; }
`;

const slideUp = keyframes`
  from { transform: translateY(20px); }
  to { transform: translateY(0); }
`;

const AnimatedDiv = styled.div`
  animation: ${fadeIn} 1s ease-in, ${slideUp} 0.5s ease-out;
`;
