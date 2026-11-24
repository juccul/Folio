import React from 'react';
import {AbsoluteFill, Easing, Img, interpolate, spring, staticFile, useCurrentFrame, useVideoConfig} from 'remotion';
import {Format} from '../components/layout';
import {Zoom} from '../components/ClipCard';
import {clips} from '../config/clips';
import trailer from '../config/trailer.json';
import {Device, deviceHeight} from './Device';
import {T, caption, gradientText, head, reveal, stage, tail} from './style';

export type Scene = {
  id: string;
  type: string;
  len: number;
  clip?: string;
  trim?: number;
  lines?: string[];
  warm?: number;
  title?: string;
  sub?: string;
  word?: string;
  side?: string;
  reveal?: number; // local frame the words land (on the snare)
  at?: number[]; // stats: local frames each line lands
};

// Where the full-window captures need a closer look to stay legible.
const ZOOMS: Record<string, {wide: Zoom[]; tall: Zoom[]}> = {
  editor: {
    wide: [{f: 0, x: 0.62, y: 0.4, s: 1.2}, {f: 52, x: 0.62, y: 0.4, s: 1.2}, {f: 76, x: 0.42, y: 0.3, s: 1.65}],
    tall: [{f: 0, x: 0.72, y: 0.4, s: 1.45}, {f: 52, x: 0.72, y: 0.4, s: 1.45}, {f: 76, x: 0.4, y: 0.3, s: 1.95}],
  },
  solveDark: {
    wide: [{f: 0, x: 0.3, y: 0.3, s: 1.5}, {f: 40, x: 0.3, y: 0.3, s: 1.5}, {f: 64, x: 0.62, y: 0.4, s: 1.25}, {f: 120, x: 0.8, y: 0.42, s: 1.6}],
    tall: [{f: 0, x: 0.3, y: 0.3, s: 1.8}, {f: 44, x: 0.3, y: 0.3, s: 1.8}, {f: 72, x: 0.82, y: 0.4, s: 1.95}],
  },
  library: {
    wide: [{f: 0, x: 0.55, y: 0.35, s: 1.25}, {f: 60, x: 0.55, y: 0.35, s: 1.25}, {f: 84, x: 0.6, y: 0.36, s: 1.5}],
    tall: [{f: 0, x: 0.6, y: 0.33, s: 1.5}, {f: 60, x: 0.6, y: 0.33, s: 1.5}, {f: 84, x: 0.6, y: 0.36, s: 1.85}],
  },
};
const zoomsFor = (clip: string, format: Format) => {
  const z = ZOOMS[clip];
  if (!z) return [];
  return format === '16:9' ? z.wide : z.tall;
};

const Label: React.FC<{format: Format; opacity?: number}> = ({format, opacity = 1}) => {
  const st = stage(format);
  return (
    <div
      style={{
        position: 'absolute',
        left: 0,
        width: st.vertical && format === '9:16' ? 960 : st.w,
        top: format === '9:16' ? 1236 : st.h - 56,
        textAlign: 'center',
        fontFamily: T.font,
        fontSize: 18,
        fontWeight: 500,
        color: '#6e6e73',
        opacity,
      }}
    >
      {trailer.label}
    </div>
  );
};

// A soft pool of light behind the product.
const Glow: React.FC<{x: number; y: number; r: number; warm?: boolean; k?: number}> = ({x, y, r, warm, k = 1}) => (
  <div
    style={{
      position: 'absolute',
      left: x - r,
      top: y - r,
      width: r * 2,
      height: r * 2,
      borderRadius: '50%',
      background: warm
        ? 'radial-gradient(circle, rgba(242,178,58,0.22), rgba(234,124,44,0.06) 45%, transparent 70%)'
        : 'radial-gradient(circle, rgba(170,180,255,0.14), rgba(120,120,140,0.05) 45%, transparent 70%)',
      opacity: k,
    }}
  />
);

// Opening: an extreme close-up of the screen as the answer writes itself, pulling back to reveal the device.
export const Macro: React.FC<{s: Scene; format: Format}> = ({s, format}) => {
  const frame = useCurrentFrame();
  const st = stage(format);
  const k = interpolate(frame, [0, s.len], [0, 1], {easing: Easing.bezier(0.33, 0, 0.2, 1)});
  const base = st.vertical ? st.w * 1.05 : st.w * 0.92;
  const scale = interpolate(k, [0, 1], [2.5, 1.0]);
  const rotX = interpolate(k, [0, 1], [0, 8]);
  const rotY = interpolate(k, [0, 1], [-4, -10]);
  const focus = interpolate(k, [0, 1], [1, 0]);
  const dh = deviceHeight(base);
  return (
    <AbsoluteFill style={{background: T.black, perspective: 2400, opacity: tail(frame, s.len, 6)}}>
      <Glow x={st.cx} y={st.cy} r={base * 0.8} warm k={1 - focus * 0.6} />
      <div
        style={{
          position: 'absolute',
          left: st.cx - base / 2,
          top: st.cy - dh / 2,
          transformStyle: 'preserve-3d',
          // Aim the close-up at the sum on the page, then glide back to the whole device.
          transform: `translate(${focus * base * (st.vertical ? 0.22 : 0.04)}px, ${focus * base * 0.03}px) scale(${scale})`,
        }}
      >
        <Device src={clips[s.clip!].src} startFrom={s.trim} width={base} rotX={rotX} rotY={rotY} reflection={false} />
      </div>
      <AbsoluteFill style={{background: 'radial-gradient(ellipse at center, transparent 78%, rgba(0,0,0,0.35) 100%)'}} />
      <Label format={format} opacity={interpolate(frame, [10, 24], [0, 1], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'})} />
    </AbsoluteFill>
  );
};

// Full-screen type on black.
export const TypeCard: React.FC<{s: Scene; format: Format}> = ({s, format}) => {
  const frame = useCurrentFrame();
  const st = stage(format);
  const size = format === '16:9' ? 112 : format === '9:16' ? 88 : 84;
  return (
    <AbsoluteFill style={{background: T.black}}>
      <div
        style={{
          position: 'absolute',
          left: 0,
          width: format === '9:16' ? 960 : st.w,
          top: st.cy - size * 1.2,
          textAlign: 'center',
          opacity: tail(frame, s.len, 7),
        }}
      >
        {s.lines!.map((line, i) => (
          <div
            key={line}
            style={{
              ...head(size),
              ...reveal(frame, i * 9),
            }}
          >
            {line}
          </div>
        ))}
      </div>
    </AbsoluteFill>
  );
};

// The name, lit by a passing sweep of light.
export const Title: React.FC<{s: Scene; format: Format}> = ({s, format}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const st = stage(format);
  const size = format === '16:9' ? 250 : 200;
  const sweep = interpolate(frame, [8, 50], [-40, 140], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});
  const icon = spring({frame, fps, config: {damping: 200, stiffness: 90}});
  return (
    <AbsoluteFill style={{background: T.black, opacity: tail(frame, s.len, 8)}}>
      <Glow x={st.cx} y={st.cy} r={size * 2.4} warm k={interpolate(frame, [0, 30], [0, 1], {extrapolateRight: 'clamp'})} />
      <div style={{position: 'absolute', left: 0, width: format === '9:16' ? 960 : st.w, top: st.cy - size * 0.95, display: 'flex', flexDirection: 'column', alignItems: 'center', gap: size * 0.08}}>
        <Img src={staticFile('brand/folio.svg')} style={{width: size * 0.42, height: size * 0.42, opacity: icon, transform: `scale(${0.85 + 0.15 * icon})`, filter: `blur(${(1 - icon) * 10}px)`}} />
        <div
          style={{
            fontFamily: T.font,
            fontWeight: 700,
            fontSize: size,
            lineHeight: 1,
            backgroundImage: `linear-gradient(100deg, rgba(255,255,255,0) ${sweep - 20}%, rgba(255,255,255,0.85) ${sweep}%, rgba(255,255,255,0) ${sweep + 20}%), ${T.warm}`,
            WebkitBackgroundClip: 'text',
            backgroundClip: 'text',
            color: 'transparent',
            ...reveal(frame, 4, 24),
            letterSpacing: '-0.04em',
          }}
        >
          Folio
        </div>
      </div>
    </AbsoluteFill>
  );
};

// The device rises and turns into view, title above.
export const DeviceScene: React.FC<{s: Scene; format: Format}> = ({s, format}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const st = stage(format);
  const clip = clips[s.clip!];
  const rise = spring({frame, fps, config: {damping: 200, stiffness: 40, mass: 1.2}});
  const drift = interpolate(frame, [0, s.len], [0, 1]);
  const w = st.deviceW;
  const dh = deviceHeight(w);
  const rotX = 55 * (1 - rise) + 10 - drift * 4;
  const rotY = -24 * (1 - rise) - 6 + drift * 10;
  const titleSize = format === '16:9' ? 80 : 64;
  return (
    <AbsoluteFill style={{background: T.black, opacity: tail(frame, s.len, 7)}}>
      <Glow x={st.cx} y={st.deviceY} r={w * 0.75} warm={!!s.warm} k={rise} />
      <div style={{position: 'absolute', left: 0, width: format === '9:16' ? 960 : st.w, top: st.titleY, textAlign: 'center'}}>
        <div style={{...head(titleSize), ...reveal(frame, 10)}}>{s.title}</div>
        {s.sub ? <div style={{...caption(titleSize * 0.45), marginTop: 14, ...reveal(frame, 18)}}>{s.sub}</div> : null}
      </div>
      <div
        style={{
          position: 'absolute',
          left: st.cx - w / 2,
          top: st.deviceY - dh / 2 + (1 - rise) * 260 + (format === '16:9' ? 60 : 40),
          perspective: 2200,
          transformStyle: 'preserve-3d',
          opacity: Math.min(1, rise * 1.6),
          transform: `scale(${0.92 + drift * 0.06})`,
        }}
      >
        <Device src={clip.src} startFrom={s.trim} width={w} rotX={rotX} rotY={rotY} zooms={zoomsFor(s.clip!, format)} dark={clip.dark} />
      </div>
      <Label format={format} />
    </AbsoluteFill>
  );
};

// One big word beside the device turned toward it.
export const Split: React.FC<{s: Scene; format: Format}> = ({s, format}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const st = stage(format);
  const clip = clips[s.clip!];
  const enter = spring({frame, fps, config: {damping: 200, stiffness: 70}});
  const drift = interpolate(frame, [0, s.len], [0, 1]);
  const right = s.side === 'right';
  const w = st.splitDeviceW;
  const dh = deviceHeight(w);
  const wordSize = format === '16:9' ? 150 : 118;
  const dir = right ? 1 : -1; // which side the device sits on (16:9)
  let deviceX: number;
  let deviceY: number;
  let rotY: number;
  if (!st.vertical) {
    deviceX = right ? 110 : st.w - 110 - w;
    deviceX += (1 - enter) * dir * -260;
    deviceY = st.cy - dh / 2 + 20;
    rotY = dir * (26 * (1 - enter) + 16 - drift * 6);
  } else {
    deviceX = st.cx - w / 2;
    deviceY = st.deviceY - dh / 2 + 40 + (1 - enter) * 200;
    rotY = dir * (14 * (1 - enter) + 6 - drift * 5);
  }
  const textBox: React.CSSProperties = !st.vertical
    ? {left: right ? st.w - 130 - 600 : 130, width: 600, top: st.cy - wordSize * 0.9, textAlign: right ? 'right' : 'left'}
    : {left: 0, width: format === '9:16' ? 960 : st.w, top: st.titleY, textAlign: 'center'};
  return (
    <AbsoluteFill style={{background: T.black, opacity: tail(frame, s.len, 6)}}>
      <Glow x={deviceX + w / 2} y={deviceY + dh / 2} r={w * 0.7} k={enter} />
      <div style={{position: 'absolute', ...textBox}}>
        <div style={{...head(wordSize), ...reveal(frame, s.reveal ?? 6, 16)}}>{s.word}</div>
        <div style={{...caption(wordSize * 0.24), marginTop: 20, ...reveal(frame, (s.reveal ?? 6) + 8)}}>{s.sub}</div>
      </div>
      <div
        style={{
          position: 'absolute',
          left: deviceX,
          top: deviceY,
          perspective: 2000,
          transformStyle: 'preserve-3d',
          opacity: Math.min(1, enter * 1.5),
          transform: `scale(${0.96 + drift * 0.05})`,
        }}
      >
        <Device src={clip.src} startFrom={s.trim} width={w} rotX={6 + (1 - enter) * 10} rotY={rotY} dark={clip.dark} />
      </div>
      <Label format={format} />
    </AbsoluteFill>
  );
};

// Three short promises, landing one per two beats.
export const Stats: React.FC<{s: Scene; format: Format}> = ({s, format}) => {
  const frame = useCurrentFrame();
  const st = stage(format);
  const big = format === '16:9' ? 84 : 80;
  const row = !st.vertical;
  return (
    <AbsoluteFill style={{background: T.black, opacity: tail(frame, s.len, 8)}}>
      <div
        style={{
          position: 'absolute',
          left: row ? 120 : 60,
          width: row ? st.w - 240 : 840,
          top: row ? st.cy - 110 : format === '9:16' ? 320 : 150,
          display: 'flex',
          flexDirection: row ? 'row' : 'column',
          justifyContent: 'space-between',
          gap: row ? 40 : 64,
          textAlign: 'center',
        }}
      >
        {trailer.stats.map((x, i) => (
          <div key={x.big} style={{flex: 1}}>
            <div style={{...head(big), ...reveal(frame, (s.at ? s.at[i] : 4 + i * 30), 14)}}>{x.big}</div>
            <div style={{...caption(30), marginTop: 18, ...reveal(frame, (s.at ? s.at[i] : 4 + i * 30) + 6)}}>{x.sub}</div>
          </div>
        ))}
      </div>
    </AbsoluteFill>
  );
};

export const End: React.FC<{s: Scene; format: Format}> = ({s, format}) => {
  const frame = useCurrentFrame();
  const st = stage(format);
  const e = trailer.end;
  const size = format === '16:9' ? 150 : 130;
  const out = interpolate(frame, [s.len - 24, s.len - 4], [1, 0], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});
  return (
    <AbsoluteFill style={{background: T.black, opacity: out}}>
      <Glow x={st.cx} y={st.cy} r={size * 3} k={interpolate(frame, [0, 40], [0, 0.8], {extrapolateRight: 'clamp'})} />
      <div
        style={{
          position: 'absolute',
          left: 0,
          width: format === '9:16' ? 960 : st.w,
          top: st.cy - size * 1.5,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          textAlign: 'center',
        }}
      >
        <Img src={staticFile('brand/folio.svg')} style={{width: size * 0.62, height: size * 0.62, ...reveal(frame, 0, 22), letterSpacing: undefined}} />
        <div style={{...head(size), ...gradientText(T.warm), ...reveal(frame, 6, 22), letterSpacing: '-0.035em', marginTop: size * 0.12}}>{e.name}</div>
        <div style={{...head(size * 0.26), marginTop: size * 0.18, ...reveal(frame, 18)}}>{e.line}</div>
        <div style={{...caption(size * 0.22), marginTop: size * 0.3, ...reveal(frame, 30)}}>{e.cta}</div>
        <div style={{...caption(size * 0.22), color: T.white, marginTop: size * 0.06, ...reveal(frame, 36)}}>{e.url}</div>
      </div>
    </AbsoluteFill>
  );
};
