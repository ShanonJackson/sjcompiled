import { transform } from '../../test-utils';

describe('css', () => {
  it('should remove css node if assigned to a variable', () => {
    const actual = transform(`
      import { css } from '@sjcompiled/react';

      const styles = css\`color: red;\`;
    `);

    expect(actual).toInclude('const styles = null;');
  });
});
