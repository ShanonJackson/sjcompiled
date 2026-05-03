import '@compiled/react';

const baseStyles = { color: 'blue', fontSize: '14px' };

const MyComponent = () => (
  <div css={{ ...baseStyles, fontWeight: 'bold' }}>Hello</div>
);
