import { styled } from '@compiled/react';

const BaseButton = (props) => <button {...props} />;

const StyledBaseButton = styled(BaseButton)`
  color: red;
  font-weight: bold;
`;
