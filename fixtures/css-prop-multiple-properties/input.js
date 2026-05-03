import * as React from 'react';
import { css } from '@compiled/react';

const MyComponent = () => (
  <div css={{
    color: 'blue',
    fontSize: '16px',
    fontWeight: 'bold',
    margin: '10px',
    padding: '20px',
    border: '1px solid black',
    borderRadius: '4px',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  }}>
    Hello
  </div>
);
