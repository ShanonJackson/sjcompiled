import { ClassNames } from '@compiled/react';

const MyComponent = () => (
  <ClassNames>
    {({ css, style }) => (
      <div className={css({ color: 'blue' })} style={style}>Hello</div>
    )}
  </ClassNames>
);
