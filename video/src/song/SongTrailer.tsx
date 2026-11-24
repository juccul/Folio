import React from 'react';
import {AbsoluteFill, Audio, Easing, Sequence, interpolate, staticFile, useCurrentFrame} from 'remotion';
import {loadFonts} from '../components/fonts';
import {Format} from '../components/layout';
import {clips} from '../config/clips';
import edit from '../config/song-edit.json';
import song from '../config/song.json';
import trailer from '../config/trailer.json';
import {Device, deviceHeight} from '../trailer/Device';
import {DeviceScene, End, Scene, Split, Stats, Title, TypeCard} from '../trailer/scenes';
import {T, head, reveal, stage, tail} from '../trailer/style';

loadFonts();

const FPS = edit.fps;
const BEAT_S = 60 / edit.bpm;
// Frames are snapped from absolute song time, so cuts never drift off the grid.
export const barF = (bar: number) => Math.round(bar * 4 * BEAT_S * FPS);
export const beatF = (beats: number) => Math.round(beats * BEAT_S * FPS);
const lastBar = Math.max(...edit.sections.map((s) => s.to));
type Cut = 'full' | 'teaser';
export const songTrailerLength = (cut: Cut = 'full') => barF(cut === 'teaser' ? edit.teaser.endBar + 2 : lastBar);

type EditScene = Scene & {
  bar: number;
  bars: number;
  focus?: number[];
  s0?: number;
  s1?: number;
  clipDelay?: number;
  words?: string[];
};

// A camera pressed against the glass: the device screen fills the frame, drifting in or out.
const MacroShot: React.FC<{s: EditScene; len: number; format: Format}> = ({s, len, format}) => {
  const frame = useCurrentFrame();
  const st = stage(format);
  const base = st.vertical ? st.w * 1.0 : st.w * 0.92;
  const dh = deviceHeight(base);
  const k = interpolate(frame, [0, len], [0, 1], {easing: Easing.bezier(0.33, 0, 0.67, 1)});
  const scale = (s.s0 ?? 2) + ((s.s1 ?? 2) - (s.s0 ?? 2)) * k;
  const [fx, fy] = s.focus ?? [0.5, 0.5];
  const sw = base;
  const sh = sw / 1.6;
  const tx = -(fx - 0.5) * sw * scale;
  const ty = -(fy - 0.5) * sh * scale;
  const clip = clips[s.clip!];
  const delay = s.clipDelay ?? 0;
  return (
    <AbsoluteFill style={{background: T.black, perspective: 2400, opacity: tail(frame, len, 3)}}>
      <div style={{position: 'absolute', left: st.w / 2 - base / 2, top: st.h / 2 - dh / 2, transform: `translate(${tx}px, ${ty}px) scale(${scale})`}}>
        {/* The clip starts after `delay` frames on the blank page, so the answer lands on the drop. */}
        <Sequence from={delay} layout="none">
          <Device src={clip.src} startFrom={s.trim} width={base} rotX={0} rotY={-3 + k * 2} reflection={false} sweepAt={-100} />
        </Sequence>
        {delay > 0 && frame < delay ? <Device src={clip.src} startFrom={s.trim} width={base} rotX={0} rotY={-3} reflection={false} sweepAt={-100} paused /> : null}
      </div>
      <AbsoluteFill style={{background: 'radial-gradient(ellipse at center, transparent 70%, rgba(0,0,0,0.35) 100%)'}} />
    </AbsoluteFill>
  );
};

// The drop: the answer writes itself, and the camera pulls back off the glass to the whole device.
const PullBack: React.FC<{s: EditScene; len: number; format: Format}> = ({s, len, format}) => {
  const frame = useCurrentFrame();
  const st = stage(format);
  const base = st.vertical ? st.deviceW : st.deviceW * 0.92;
  const dh = deviceHeight(base);
  const k = interpolate(frame, [0, len * 0.8], [0, 1], {extrapolateRight: 'clamp', easing: Easing.bezier(0.16, 1, 0.3, 1)});
  const startScale = (st.vertical ? st.w * 1.0 : st.w * 0.92) / base * 2.5;
  const scale = startScale + (1 - startScale) * k;
  const [fx, fy] = s.focus ?? [0.5, 0.5];
  const sw = base;
  const sh = sw / 1.6;
  const tx = -(fx - 0.5) * sw * scale * (1 - k);
  const ty = -(fy - 0.5) * sh * scale * (1 - k);
  const cy = st.vertical ? st.deviceY + 60 : st.deviceY + 50;
  const y = (st.h / 2) * (1 - k) + cy * k;
  const size = format === '16:9' ? 80 : 64;
  const snare = beatF(1);
  return (
    <AbsoluteFill style={{background: T.black, perspective: 2200, opacity: tail(frame, len, 5)}}>
      <div
        style={{
          position: 'absolute',
          left: st.w / 2 - base / 2,
          top: y - dh / 2,
          transformStyle: 'preserve-3d',
          transform: `translate(${tx}px, ${ty}px) scale(${scale})`,
        }}
      >
        <Device src={clips[s.clip!].src} startFrom={s.trim} width={base} rotX={10 * k} rotY={-8 * k} reflection={k > 0.6} sweepAt={snare + 6} />
      </div>
      <div style={{position: 'absolute', left: 0, width: format === '9:16' ? 960 : st.w, top: st.titleY, textAlign: 'center'}}>
        {s.lines!.map((line, i) => (
          <div key={line} style={{...head(size), ...reveal(frame, snare + i * 6, 14)}}>
            {line}
          </div>
        ))}
      </div>
    </AbsoluteFill>
  );
};

// One word per beat, hard cuts, no easing: the fastest the copy ever moves.
const Burst: React.FC<{s: EditScene; format: Format}> = ({s, format}) => {
  const frame = useCurrentFrame();
  const st = stage(format);
  const i = Math.min(s.words!.length - 1, Math.floor(frame / beatF(1)));
  const local = frame - beatF(i);
  const size = format === '16:9' ? 150 : 120;
  return (
    <AbsoluteFill style={{background: T.black}}>
      <div
        style={{
          position: 'absolute',
          left: 0,
          width: format === '9:16' ? 960 : st.w,
          top: st.cy - size * 0.6,
          textAlign: 'center',
          ...head(size),
          opacity: interpolate(local, [0, 2], [0.4, 1], {extrapolateRight: 'clamp'}),
          transform: `scale(${interpolate(local, [0, 4], [1.04, 1], {extrapolateRight: 'clamp'})})`,
        }}
      >
        {s.words![i]}
      </div>
    </AbsoluteFill>
  );
};

const VIEW = (s: EditScene, len: number, format: Format) => {
  const sc = {...s, len};
  switch (s.type) {
    case 'macro':
      return <MacroShot s={s} len={len} format={format} />;
    case 'pullback':
      return <PullBack s={s} len={len} format={format} />;
    case 'burst':
      return <Burst s={s} format={format} />;
    case 'type':
      return <TypeCard s={sc} format={format} />;
    case 'title':
      return <Title s={sc} format={format} />;
    case 'device':
      return <DeviceScene s={sc} format={format} />;
    case 'split':
      // Words land on the snare (beat 2), after the cut on the downbeat.
      return <Split s={{...sc, reveal: beatF(1) - 2}} format={format} />;
    case 'stats':
      // Promises land on the snares: beats 2, 4 and 6 of the section.
      return <Stats s={{...sc, at: [beatF(1), beatF(3), beatF(5)]}} format={format} />;
    case 'end':
      return <End s={sc} format={format} />;
    default:
      return null;
  }
};

export const SongTrailer: React.FC<{format: Format; cut?: Cut}> = ({format, cut = 'full'}) => {
  const teaser = cut === 'teaser';
  const scenes = (teaser ? edit.teaser.scenes : edit.scenes) as EditScene[];
  const endBar = teaser ? edit.teaser.endBar : edit.sections.find((x) => x.name === 'end')!.from;
  const stop = barF(endBar);
  // The teaser enters the song a few bars before the drop.
  const startFrame = Math.round(song.startAt * FPS) + (teaser ? barF(edit.teaser.songBar) : 0);
  return (
    <AbsoluteFill style={{background: '#000'}}>
      {scenes.map((s) => {
        const from = barF(s.bar);
        const len = barF(s.bar + s.bars) - from;
        return (
          <Sequence key={s.id} from={from} durationInFrames={len}>
            {VIEW(s, len, format)}
          </Sequence>
        );
      })}
      {/* The song plays from the bar that puts its drop on the edit's drop, then stops dead
          on the end card's downbeat: a hard stop, then a single chime over the lockup. */}
      <Audio
        src={staticFile(song.src)}
        startFrom={startFrame}
        volume={(f) =>
          Math.min(
            interpolate(f, [0, 3], [0, 1], {extrapolateRight: 'clamp'}),
            interpolate(f, [stop + 5, stop + 14], [1, 0], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'}),
          )
        }
      />
      {teaser ? null : (
        <Sequence from={0} durationInFrames={45}>
          <Audio src={staticFile('audio/pen.wav')} volume={0.5} />
        </Sequence>
      )}
      <Sequence from={stop}>
        <Audio src={staticFile('audio/chime.wav')} volume={0.7} />
      </Sequence>
    </AbsoluteFill>
  );
};

export {trailer};
