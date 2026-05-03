import '@compiled/react';

const MyComponent = () => (
  <div css={{
    color: 'blue',
    '@media (max-width: 768px)': {
      color: 'red',
    },
  }}>
    Hello
  </div>
);
