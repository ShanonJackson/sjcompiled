import { ClassNames } from '@compiled/react';

const MyComponent = ({ isActive }) => (
  <ClassNames>
    {({ css }) => (
      <div className={css({ color: isActive ? 'green' : 'gray' })}>Status</div>
    )}
  </ClassNames>
);
