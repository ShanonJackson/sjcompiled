import { cssMap } from '@compiled/react';

const styles = cssMap({
  default: {
    color: 'black',
    '&:hover': {
      color: 'blue',
    },
  },
  active: {
    color: 'blue',
    '&:hover': {
      color: 'darkblue',
    },
  },
});
