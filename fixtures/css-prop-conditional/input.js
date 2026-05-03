import '@compiled/react';

const MyComponent = ({ isPrimary }) => (
  <div css={{ color: isPrimary ? 'blue' : 'red' }}>Hello</div>
);
