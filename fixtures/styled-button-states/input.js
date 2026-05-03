import { styled } from '@compiled/react';

const Button = styled.button({
  padding: '10px 20px',
  backgroundColor: 'blue',
  color: 'white',
  border: 'none',
  cursor: 'pointer',
  '&:hover': {
    backgroundColor: 'darkblue',
  },
  '&:active': {
    backgroundColor: 'navy',
  },
  '&:disabled': {
    backgroundColor: 'gray',
    cursor: 'not-allowed',
  },
});
