import React from 'react';
import {Easing, OffthreadVideo, interpolate, staticFile, useCurrentFrame} from 'remotion';
import {Zoom} from '../components/ClipCard';

type Props = {
  src: string;
  width: number;
  startFrom?: number;
  rotX: number;
  rotY: number;
  rotZ?: number;
  zooms?: Zoom[];
  reflection?: boolean;
  dark?: boolean;
  switchOn?: number; // local frame the screen lights up
  sweepAt?: number; // local frame a strip of light starts travelling along the edge
  paused?: boolean; // show the clip's first frame without playing (a held blank page)
};

const zoomAt = (frame: number, zooms: Zoom[]) => {
  if (!zooms.length) return {x: 0.5, y: 0.5, s: 1};
  let cur = {x: zooms[0].x, y: zooms[0].y, s: zooms[0].s};
  for (let i = 1; i < zooms.length; i++) {
    const a = zooms[i - 1];
    const b = zooms[i];
    if (frame <= a.f) break;
    const k = interpolate(frame, [a.f, b.f], [0, 1], {
      extrapolateLeft: 'clamp',
      extrapolateRight: 'clamp',
      easing: Easing.bezier(0.65, 0, 0.35, 1),
    });
    cur = {x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k, s: a.s + (b.s - a.s) * k};
  }
  return cur;
};

// The app's window floating in a dark void: glass sheen that follows its turn, a strip of light
// along its edge, a soft glow and contact shadow, and the real app playing on its 16:10 screen.
export const Device: React.FC<Props> = ({src, width, startFrom = 0, rotX, rotY, rotZ = 0, zooms = [], reflection = true, dark, switchOn, sweepAt = 6, paused}) => {
  const frame = useCurrentFrame();
  // No hardware: the app's own window floats in the void.
  const bezel = 0;
  const sw = width;
  const sh = sw / 1.6;
  const height = sh;
  const radius = width * 0.018;
  const z = zoomAt(frame, zooms);
  const clamp = (v: number, m: number) => Math.max(-m, Math.min(m, v));
  const tx = clamp((0.5 - z.x) * sw * z.s, Math.max(0, (sw * z.s - sw) / 2));
  const ty = clamp((0.5 - z.y) * sh * z.s, Math.max(0, (sh * z.s - sh) / 2));
  const sheen = 50 + rotY * 3;
  // The screen is the brightest thing in frame; it switches on over ~0.4 s.
  const on = switchOn === undefined ? 1 : interpolate(frame, [switchOn, switchOn + 12], [0, 1], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp', easing: Easing.out(Easing.quad)});
  // A specular strip travels along the edge over ~1.6 s.
  const sweep = interpolate(frame, [sweepAt, sweepAt + 48], [-30, 130], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp', easing: Easing.inOut(Easing.sin)});
  const screen = (
    <div
      style={{
        position: 'absolute',
        left: bezel,
        top: bezel,
        width: sw,
        height: sh,
        borderRadius: radius,
        overflow: 'hidden',
        background: dark ? '#0e0e0e' : '#fff',
        filter: on < 1 ? `brightness(${0.05 + 0.95 * on})` : undefined,
        boxShadow: `0 0 ${width * 0.06}px rgba(255,255,255,${0.10 * on})`,
      }}
    >
      <div style={{width: sw, height: sh, transform: `translate(${tx}px, ${ty}px) scale(${z.s})`}}>
        {paused ? (
          <div style={{width: '100%', height: '100%', background: dark ? '#0e0e0e' : '#fff'}} />
        ) : (
          <OffthreadVideo src={staticFile(src)} startFrom={startFrom} muted style={{width: '100%', height: '100%'}} />
        )}
      </div>
      <div
        style={{
          position: 'absolute',
          inset: 0,
          background: `linear-gradient(115deg, rgba(255,255,255,0) ${sheen - 18}%, rgba(255,255,255,0.10) ${sheen}%, rgba(255,255,255,0) ${sheen + 14}%)`,
        }}
      />
    </div>
  );
  const body = (key: string, mirrored = false) => (
    <div
      key={key}
      style={{
        position: 'absolute',
        inset: 0,
        transformStyle: 'preserve-3d',
        transform: mirrored ? 'scaleY(-1)' : undefined,
      }}
    >
      {screen}
      <div
        style={{
          position: 'absolute',
          inset: 0,
          borderRadius: radius,
          padding: Math.max(2, width * 0.0025),
          background: `linear-gradient(120deg, rgba(255,255,255,0) ${sweep - 12}%, rgba(255,255,255,0.85) ${sweep}%, rgba(255,255,255,0) ${sweep + 12}%)`,
          WebkitMask: 'linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0)',
          WebkitMaskComposite: 'xor',
          maskComposite: 'exclude',
        }}
      />
    </div>
  );
  return (
    <div style={{position: 'relative', width, height, transformStyle: 'preserve-3d'}}>
      <div
        style={{
          position: 'absolute',
          inset: 0,
          transformStyle: 'preserve-3d',
          transform: `rotateX(${rotX}deg) rotateY(${rotY}deg) rotateZ(${rotZ}deg)`,
        }}
      >
        {body('front')}
      </div>
      <div
        style={{
          position: 'absolute',
          left: width * 0.1,
          width: width * 0.8,
          top: height * 0.98,
          height: width * 0.05,
          borderRadius: '50%',
          background: 'radial-gradient(ellipse at center, rgba(0,0,0,0.9), rgba(0,0,0,0) 70%)',
          filter: 'blur(6px)',
          transform: `rotateX(${rotX}deg)`,
        }}
      />
      {reflection ? (
        <div
          style={{
            position: 'absolute',
            left: 0,
            top: height + width * 0.02,
            width,
            height,
            opacity: 0.16,
            WebkitMaskImage: 'linear-gradient(180deg, rgba(0,0,0,0.9), rgba(0,0,0,0) 45%)',
            maskImage: 'linear-gradient(180deg, rgba(0,0,0,0.9), rgba(0,0,0,0) 45%)',
            transform: `rotateX(${-rotX}deg) rotateY(${rotY}deg) scaleY(-1)`,
            filter: 'blur(2px)',
          }}
        >
          <div style={{position: 'absolute', inset: 0, borderRadius: radius, background: dark ? '#222' : '#cfcfd2'}} />
        </div>
      ) : null}
    </div>
  );
};

export const deviceHeight = (width: number) => width / 1.6;
