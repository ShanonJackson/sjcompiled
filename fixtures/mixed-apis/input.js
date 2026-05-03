import { styled, ClassNames } from '@compiled/react';

const StyledHeader = styled.h1`
  font-size: 24px;
  font-weight: bold;
`;

const MyComponent = () => (
  <div>
    <StyledHeader>Title</StyledHeader>
    <div css={{ color: 'blue', padding: '10px' }}>Content</div>
    <ClassNames>
      {({ css }) => (
        <span className={css({ color: 'green' })}>Status</span>
      )}
    </ClassNames>
  </div>
);
