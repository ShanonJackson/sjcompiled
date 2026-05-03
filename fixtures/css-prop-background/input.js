import * as React from 'react';
import { css } from '@compiled/react';

const MyComponent = () => (
  <div css={{
    background: 'linear-gradient(to right, red, blue)',
    backgroundSize: 'cover',
    backgroundPosition: 'center',
  }}>
    Hello
  </div>
);
