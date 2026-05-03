import { styled } from '@compiled/react';

const StyledInput = styled.input({
  border: '1px solid #ccc',
  borderRadius: '4px',
  padding: '8px 12px',
  fontSize: '14px',
  '&:focus': {
    borderColor: 'blue',
    outline: 'none',
  },
});
