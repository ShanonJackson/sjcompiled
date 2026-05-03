import { ClassNames } from '@compiled/react';

const First = () => (
  <ClassNames>
    {({ css }) => <div className={css({ color: 'red' })}>Red</div>}
  </ClassNames>
);

const Second = () => (
  <ClassNames>
    {({ css }) => <div className={css({ color: 'blue' })}>Blue</div>}
  </ClassNames>
);
