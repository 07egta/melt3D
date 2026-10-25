import React from 'react';
import {Composition} from 'remotion';
import {Main} from './Main';
import tl from '../public/timeline.json';
export const Root: React.FC = () => (
  <Composition id="Melt3D" component={Main} durationInFrames={tl.frames} fps={30} width={1280} height={720} />
);
