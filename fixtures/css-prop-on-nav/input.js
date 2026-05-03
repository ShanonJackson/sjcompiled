import * as React from 'react';
import { css } from '@compiled/react';

const Navigation = () => (
  <nav css={{
    display: 'flex',
    gap: '16px',
    padding: '10px 20px',
    backgroundColor: '#f8f8f8',
    borderBottom: '1px solid #ddd',
  }}>
    <a href="/">Home</a>
    <a href="/about">About</a>
  </nav>
);
