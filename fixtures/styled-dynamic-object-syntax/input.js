import { styled } from '@compiled/react';

const DynamicDiv = styled.div({
  color: (props) => props.color,
  fontSize: '14px',
});
