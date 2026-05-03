import { ClassNames } from '@compiled/react';

const MyComponent = () => (
  <ClassNames>
    {({ css }) => (
      <div>
        <h1 className={css({ fontSize: '24px', fontWeight: 'bold' })}>Title</h1>
        <p className={css({ color: 'gray', lineHeight: '1.5' })}>Description</p>
      </div>
    )}
  </ClassNames>
);
