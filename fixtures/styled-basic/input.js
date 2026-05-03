import { styled } from '@compiled/react';

const StyledDiv = styled.div`
  color: blue;
  font-size: 14px;
`;

const StyledWithProps = styled.div`
  color: ${(props) => props.color};
  padding: 10px;
`;
