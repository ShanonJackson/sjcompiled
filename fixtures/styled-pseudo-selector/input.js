import { styled } from '@compiled/react';

const HoverButton = styled.button({
  color: 'blue',
  '&:hover': {
    color: 'red',
    textDecoration: 'underline',
  },
  '&:active': {
    color: 'darkred',
  },
});
