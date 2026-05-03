import { styled } from '@compiled/react';

const StyledInput = styled.input`
  border: 1px solid gray;
  padding: 8px 12px;
  border-radius: 4px;
  font-size: 14px;
  &:focus {
    border-color: blue;
    outline: none;
  }
`;
