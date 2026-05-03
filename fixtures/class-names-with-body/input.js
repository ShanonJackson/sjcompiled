import { ClassNames } from '@compiled/react';

const MyComponent = () => (
  <ClassNames>
    {({ css }) => {
      const className = css({ color: 'red', fontWeight: 'bold' });
      return <div className={className}>Hello</div>;
    }}
  </ClassNames>
);
