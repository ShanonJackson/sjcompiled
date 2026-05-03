import { ClassNames } from '@compiled/react';

const MyComponent = () => (
  <ClassNames>
    {({ css }) => (
      <div className={css('color: blue; font-size: 14px;')}>Hello</div>
    )}
  </ClassNames>
);
