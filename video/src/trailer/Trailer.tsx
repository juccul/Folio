import React from 'react';
import {AbsoluteFill, Audio, Sequence, staticFile} from 'remotion';
import {loadFonts} from '../components/fonts';
import {Format} from '../components/layout';
import trailer from '../config/trailer.json';
import {DeviceScene, End, Macro, Scene, Split, Stats, Title, TypeCard} from './scenes';

loadFonts();

type Cut = 'flagship' | 'teaser';

export const trailerLength = (cut: Cut) => (trailer.cuts[cut] as Scene[]).reduce((s, x) => s + x.len, 0);

const VIEWS: Record<string, React.FC<{s: Scene; format: Format}>> = {
  macro: Macro,
  type: TypeCard,
  title: Title,
  device: DeviceScene,
  split: Split,
  stats: Stats,
  end: End,
};

// A product-trailer cut: black stage, one idea per shot, every shot starting on the beat.
export const Trailer: React.FC<{format: Format; cut: Cut}> = ({format, cut}) => {
  const scenes = trailer.cuts[cut] as Scene[];
  let at = 0;
  return (
    <AbsoluteFill style={{background: '#000'}}>
      {scenes.map((s) => {
        const from = at;
        at += s.len;
        const View = VIEWS[s.type];
        return (
          <Sequence key={s.id} from={from} durationInFrames={s.len}>
            <View s={s} format={format} />
          </Sequence>
        );
      })}
      <Audio src={staticFile(`audio/trailer-${cut}.wav`)} />
    </AbsoluteFill>
  );
};
