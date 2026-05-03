import { ClassNames } from '@compiled/react';

const MyComponent = () => (
  <ClassNames>
    {({ css }) => (
      <div className={css({ color: 'blue' })}>Hello</div>
    )}
  </ClassNames>
);
