import * as React from 'react';
import { css } from '@compiled/react';

const Overlay = () => (
  <div css={{
    position: 'fixed',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    backgroundColor: 'rgba(0, 0, 0, 0.5)',
    zIndex: 1000,
  }}>
    Overlay
  </div>
);
