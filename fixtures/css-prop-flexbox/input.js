import * as React from 'react';
import { css } from '@compiled/react';

const FlexContainer = () => (
  <div css={{
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: '16px',
  }}>
    Content
  </div>
);
