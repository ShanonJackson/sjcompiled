import { styled } from '@compiled/react';

const Tooltip = styled.div({
  position: 'relative',
  '&::after': {
    content: '"tooltip"',
    position: 'absolute',
    top: '100%',
  },
});
