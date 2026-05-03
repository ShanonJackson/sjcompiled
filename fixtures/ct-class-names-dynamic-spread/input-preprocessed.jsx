import { ClassNames } from '@compiled/react';
const TableWrapper = ({
  selectedRows
}) => {
  const styles = {};
  selectedRows.forEach(index => {
    styles[`tbody tr:nth-child(${index + 1})`] = {
      backgroundColor: "var(--ds-background-selected, #E9F2FE)"
    };
  });
  return <ClassNames>
			{({
      css
    }) => <div className={css({
      position: 'relative',
      th: {
        height: '100%',
        verticalAlign: 'middle'
      },
      ...styles
    })}>
					content
				</div>}
		</ClassNames>;
};
export const Fixture = () => <TableWrapper selectedRows={[0, 2]} />;