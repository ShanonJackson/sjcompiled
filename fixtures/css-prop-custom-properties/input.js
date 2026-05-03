import * as React from 'react';
import { css } from '@compiled/react';

const MyComponent = () => (
  <div css={{ '--my-color': 'blue', '--my-size': '16px', color: 'var(--my-color)' }}>
    Hello
  </div>
);
