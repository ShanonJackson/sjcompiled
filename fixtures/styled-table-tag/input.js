import { styled } from '@compiled/react';

const StyledTable = styled.table({
  width: '100%',
  borderCollapse: 'collapse',
  '& th': {
    backgroundColor: '#f0f0f0',
    padding: '8px',
    textAlign: 'left',
  },
  '& td': {
    padding: '8px',
    borderBottom: '1px solid #eee',
  },
});
