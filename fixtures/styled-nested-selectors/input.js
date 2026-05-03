import { styled } from '@compiled/react';

const StyledDiv = styled.div`
  color: blue;
  & > span {
    color: red;
  }
  &:first-child {
    margin-top: 0;
  }
`;
