import React from 'react';
import { styled, css } from '@compiled/react';
const GridCard = styled.div({
  display: 'grid',
  gridRow: 'card-extra-fields / end',
  gridTemplateRows: 'var(--_1d7lvij)',
  gridTemplateColumns: 'var(--_183xcfk)',
  gridColumn: 'issue-key / end',
  padding: "var(--ds-space-250, 20px)",
  textDecorationColor: 'currentColor',
  height: "var(--ds-space-300, 24px)"
}, css({
  textDecorationColor: 'currentColor !important'
}), ({
  dense
}) => dense && css({
  paddingLeft: 28,
  paddingRight: 4
}), ({
  initialDirection
}) => initialDirection && css({
  flexDirection: 'initial',
  gridTemplateRows: 'var(--_10z77pf)',
  gridTemplateColumns: 'var(--_15mlnlx)',
  gridColumn: 'issue-type / end'
}), ({
  initialDecoration
}) => initialDecoration && css({
  textDecorationColor: 'initial'
}), ({
  importantDecoration
}) => importantDecoration && css({
  textDecorationColor: 'initial !important'
}), ({
  rowDirection
}) => rowDirection && css({
  flexDirection: 'row'
}), ({
  secondary
}) => secondary && css({
  gridRow: 'card-extra-fields / end',
  gridColumn: 'issue-key / end',
  paddingLeft: 28
}));
export const Component = () => <>
    <GridCard />
    <GridCard rowDirection />
    <GridCard initialDirection />
    <GridCard initialDecoration />
    <GridCard importantDecoration />
    <GridCard dense />
    <GridCard secondary />
  </>;