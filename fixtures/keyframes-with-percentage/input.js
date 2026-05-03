import { styled, keyframes } from '@compiled/react';

const pulse = keyframes`
  0% { transform: scale(1); }
  50% { transform: scale(1.05); }
  100% { transform: scale(1); }
`;

const PulsingDiv = styled.div`
  animation: ${pulse} 2s infinite;
`;
