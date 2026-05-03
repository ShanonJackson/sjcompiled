import * as React from 'react';
import { css } from '@compiled/react';

const MyComponent = () => (
  <div>
    <hr css={{ border: 'none', borderTop: '1px solid #ccc', margin: '20px 0' }} />
    <img css={{ maxWidth: '100%', height: 'auto' }} src="test.png" alt="test" />
  </div>
);
