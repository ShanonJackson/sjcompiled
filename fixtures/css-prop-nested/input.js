import '@compiled/react';

const MyComponent = () => (
  <div css={{
    color: 'blue',
    '&:hover': {
      color: 'red',
    },
  }}>
    Hello
  </div>
);
