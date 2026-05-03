import * as React from 'react';
import { css } from '@compiled/react';

const ScrollableDiv = () => (
  <div css={{
    overflow: 'auto',
    maxHeight: '400px',
    scrollBehavior: 'smooth',
    WebkitOverflowScrolling: 'touch',
  }}>
    Scrollable content
  </div>
);
