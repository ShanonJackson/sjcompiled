import { cssMap } from '@compiled/react';

const styles = cssMap({
  primary: { color: 'blue', fontWeight: 'bold' },
  danger: { color: 'red' },
});

const MyComponent = ({ variant }) => (
  <div xcss={styles[variant]}>Hello</div>
);
