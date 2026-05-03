import * as React from 'react';
import { css } from '@compiled/react';

const Article = () => (
  <article css={{
    fontFamily: '"Georgia", serif',
    fontSize: '18px',
    lineHeight: '1.6',
    letterSpacing: '0.02em',
    wordSpacing: '0.05em',
    textRendering: 'optimizeLegibility',
  }}>
    Content
  </article>
);
