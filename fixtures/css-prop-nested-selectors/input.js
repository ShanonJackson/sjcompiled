import '@compiled/react';

const MyComponent = () => (
  <div css={{
    color: 'blue',
    '&:hover': {
      color: 'red',
    },
    '&:focus': {
      outline: '2px solid blue',
    },
    '& > span': {
      fontWeight: 'bold',
    },
  }}>Hello</div>
);
