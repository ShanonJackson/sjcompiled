import { cssMap } from '@compiled/react';

const styles = cssMap({
  danger: {
    color: 'red',
    '&:hover': {
      color: 'darkred',
    },
  },
  success: {
    color: 'green',
    '&:hover': {
      color: 'darkgreen',
    },
  },
});
