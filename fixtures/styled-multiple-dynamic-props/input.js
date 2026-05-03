import { styled } from '@compiled/react';

const StyledBox = styled.div`
  color: ${(props) => props.color};
  background-color: ${(props) => props.bg};
  padding: 10px;
`;
