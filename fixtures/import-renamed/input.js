import { styled as createStyled, ClassNames as CN } from '@compiled/react';

const StyledDiv = createStyled.div`
  color: blue;
`;

const MyComponent = () => (
  <CN>
    {({ css }) => (
      <span className={css({ fontWeight: 'bold' })}>Bold</span>
    )}
  </CN>
);
