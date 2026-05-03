import * as React from 'react';
import { css } from '@compiled/react';

const MyComponent = () => (
  <div css={{
    transition: 'all 0.3s ease',
    transform: 'translateX(0)',
    '&:hover': {
      transform: 'translateX(10px)',
    },
  }}>
    Slide me
  </div>
);
