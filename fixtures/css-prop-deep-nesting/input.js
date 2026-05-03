import * as React from 'react';
import { css } from '@compiled/react';

const MyComponent = () => (
  <div css={{
    color: 'blue',
    '& > ul': {
      listStyle: 'none',
      '& > li': {
        padding: '4px 0',
      },
    },
  }}>
    Content
  </div>
);
