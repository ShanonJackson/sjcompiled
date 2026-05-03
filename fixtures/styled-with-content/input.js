import { styled } from '@compiled/react';

const Asterisk = styled.span({
  '::after': {
    content: '"*"',
    color: 'red',
  },
});
