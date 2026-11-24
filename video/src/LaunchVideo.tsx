import React from 'react';
import {AbsoluteFill, Audio, OffthreadVideo, Sequence, interpolate, staticFile, useCurrentFrame} from 'remotion';
import {BentoSummary} from './components/BentoSummary';
import {Beat} from './components/Beat';
import {DotGrid} from './components/DotGrid';
import {EndCard} from './components/EndCard';
import {InkWipe} from './components/InkWipe';
import {Ripple} from './components/Ripple';
import {loadFonts} from './components/fonts';
import {Format, dims, layout} from './components/layout';
import {brand} from './config/brand';
import {clips} from './config/clips';
import {flagship, teaser} from './timeline';

loadFonts();

type Props = {format: Format; cut: 'flagship' | 'teaser'; music?: string};

export const LaunchVideo: React.FC<Props> = ({format, cut, music}) => {
  const frame = useCurrentFrame();
  const tl = cut === 'teaser' ? teaser() : flagship();
  const L = layout(format);
  const {w, h} = dims[format];
  // The stage flips to dark under the wipe that opens the dark-theme scene, and back under the next.
  const darkBeat = tl.placed.find((b) => b.dark);
  const darkEnd = darkBeat ? darkBeat.from + darkBeat.duration - 6 : -1;
  const isDark = darkBeat ? frame >= darkBeat.from && frame < darkEnd : false;
  const wipes: {at: number; color: string}[] = [];
  const ctx = tl.placed.find((b) => b.key === 'context');
  if (ctx) wipes.push({at: ctx.from, color: brand.tileBottom});
  if (darkBeat) {
    wipes.push({at: darkBeat.from, color: '#0b0b0c'});
    if (darkEnd < tl.endFrom) wipes.push({at: darkEnd, color: brand.stage});
  }
  if (tl.bentoFrom >= 0) wipes.push({at: tl.bentoFrom, color: brand.answer});
  wipes.push({at: tl.endFrom, color: brand.tileBottom});
  const wipeAt = new Set(wipes.map((x) => x.at));
  const cuts = [...tl.cuts, ...(tl.bentoFrom >= 0 ? [tl.bentoFrom] : []), tl.endFrom];
  const features = tl.placed.filter((b) => b.key !== 'hook' && b.key !== 'context');
  const stepOf = (key: string) => {
    const i = features.findIndex((b) => b.key === key);
    return i < 0 || cut === 'teaser' ? undefined : `${String(i + 1).padStart(2, '0')} / ${String(features.length).padStart(2, '0')}`;
  };
  return (
    <AbsoluteFill>
      <DotGrid dark={isDark} pulseAt={cuts} />
      <Ripple x={L.card.left + L.card.width / 2} y={L.card.top + L.card.height / 2} at={cuts.filter((c) => !wipeAt.has(c))} dark={isDark} />
      {tl.placed.map((b, i) => (
        <Sequence key={b.key} from={b.from} durationInFrames={b.duration}>
          <Beat
            format={format}
            duration={b.duration}
            headline={b.headline}
            accent={b.accent}
            sub={b.sub}
            clip={clips[b.clip]}
            zooms={format === '16:9' ? b.zooms : b.zoomsV ?? b.zooms}
            startFrom={b.trimStart}
            label={b.label}
            underline={b.underline}
            dark={b.dark}
            first={i === 0}
            step={stepOf(b.key)}
          />
        </Sequence>
      ))}
      {tl.bentoFrom >= 0 ? (
        <Sequence from={tl.bentoFrom} durationInFrames={tl.bento + 6}>
          <BentoSummary format={format} duration={tl.bento + 6} />
        </Sequence>
      ) : null}
      <Sequence from={tl.endFrom}>
        <EndCard format={format} />
      </Sequence>
      {wipes.map((x) => (
        <InkWipe key={x.at} at={x.at} color={x.color} />
      ))}
      {cuts.map((c) => (
        <Sequence key={`w${c}`} from={Math.max(0, c - 9)} durationInFrames={30}>
          <Audio src={staticFile(wipeAt.has(c) ? 'audio/swoosh-big.wav' : 'audio/whoosh.wav')} volume={0.55} />
        </Sequence>
      ))}
      {tl.bentoFrom >= 0 ? (
        <Sequence from={tl.bentoFrom} durationInFrames={60}>
          <Audio src={staticFile('audio/impact.wav')} volume={0.5} />
        </Sequence>
      ) : null}
      <Sequence from={tl.endFrom}>
        <Audio src={staticFile('audio/impact.wav')} volume={0.5} />
        <Audio src={staticFile('audio/chime.wav')} volume={0.45} />
      </Sequence>
      {music ? (
        <Audio
          src={staticFile(music)}
          volume={(f) => interpolate(f, [0, 3, tl.total - 20, tl.total], [0.9, 1, 1, 0], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'})}
        />
      ) : null}
    </AbsoluteFill>
  );
};

// Silent homepage loop: the product alone, no type, seamless ends.
const HERO = ['math', 'encircle', 'convert', 'search'];
export const heroLength = () => HERO.reduce((s, k) => s + clips[k].frames - 4, 0);
export const HeroLoop: React.FC = () => {
  let t = 0;
  const items = HERO.map((k) => {
    const d = clips[k].frames - 4;
    const it = {k, from: t, d};
    t += d;
    return it;
  });
  return (
    <AbsoluteFill style={{background: '#fff'}}>
      {items.map((it) => (
        <Sequence key={it.k} from={it.from} durationInFrames={it.d}>
          <OffthreadVideo src={staticFile(clips[it.k].src)} muted style={{width: '100%', height: '100%', objectFit: 'cover'}} />
        </Sequence>
      ))}
    </AbsoluteFill>
  );
};
