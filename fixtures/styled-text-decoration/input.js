import { styled } from '@compiled/react';

const StyledLink = styled.a({
  color: 'blue',
  textDecoration: 'none',
  '&:hover': {
    textDecoration: 'underline',
  },
  '&:visited': {
    color: 'purple',
  },
});
