import { styled } from '@compiled/react';

const List = styled.ul({
  listStyle: 'none',
  padding: 0,
  '& > li': {
    padding: '8px',
    borderBottom: '1px solid #eee',
  },
  '& > li:last-child': {
    borderBottom: 'none',
  },
});
