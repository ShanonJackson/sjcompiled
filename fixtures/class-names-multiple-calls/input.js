import { ClassNames } from '@compiled/react';

const MyComponent = () => (
  <ClassNames>
    {({ css }) => (
      <div>
        <span className={css({ color: 'red' })}>Red</span>
        <span className={css({ color: 'blue' })}>Blue</span>
      </div>
    )}
  </ClassNames>
);
