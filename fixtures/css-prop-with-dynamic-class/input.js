import * as React from 'react';
import { css } from '@compiled/react';

const MyComponent = ({ className }) => (
  <div className={className} css={{ color: 'blue' }}>
    Hello
  </div>
);
