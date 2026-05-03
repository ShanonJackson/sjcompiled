/** @jsx jsx */
import { jsx, cssMap } from '@atlaskit/css';
import Textfield from '@atlaskit/textfield';
const textFieldStyle = cssMap({
  invalid: {
    color: "var(--ds-text-danger, #AE2E24)"
  },
  valid: {
    color: "var(--ds-text, #292A2E)"
  }
});
type Props = {
  isValid: boolean;
};
export const Example = ({
  isValid
}: Props) => <Textfield css={[isValid ? textFieldStyle.valid : textFieldStyle.invalid]} />;