import React from 'react';
import {Easing, OffthreadVideo, interpolate, staticFile, useCurrentFrame} from 'remotion';
import {brand, ease} from '../config/brand';

export type Zoom = {f: number; x: number; y: number; s: number}; // focus point in 0..1 of the clip, scale

type Props = {
  src: string;
  width: number;
  height: number;
  aspect: number; // clip width / height
  dark?: boolean;
  zooms?: Zoom[];
  startFrom?: number;
  label?: string;
  fit?: 'cover' | 'contain';
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
      easing: Easing.bezier(...ease.snap),
    });
    cur = {x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k, s: a.s + (b.s - a.s) * k};
  }
  return cur;
};

// The real app, recorded headlessly, in a quiet window frame.
export const ClipCard: React.FC<Props> = ({src, width, height, aspect, dark, zooms = [], startFrom = 0, label, fit = 'cover'}) => {
  const frame = useCurrentFrame();
  const z = zoomAt(frame, zooms);
  // Size the video to cover (or fit) the card, then scale about the focus point.
  const cardAspect = width / height;
  const cover = fit === 'cover' ? cardAspect > aspect : cardAspect < aspect;
  const vw = cover ? width : height * aspect;
  const vh = cover ? width / aspect : height;
  const vx = (width - vw) / 2;
  const vy = (height - vh) / 2;
  // Bring the focus point to the centre without revealing the clip's edges.
  const clamp = (v: number, m: number) => Math.max(-m, Math.min(m, v));
  const tx = clamp((0.5 - z.x) * vw * z.s, Math.max(0, (vw * z.s - width) / 2));
  const ty = clamp((0.5 - z.y) * vh * z.s, Math.max(0, (vh * z.s - height) / 2));
  return (
    <div
      style={{
        position: 'relative',
        width,
        height,
        borderRadius: brand.radius,
        overflow: 'hidden',
        background: dark ? brand.darkPaper : brand.paper,
        boxShadow: dark ? '0 30px 80px rgba(0,0,0,0.55)' : brand.shadow,
        outline: dark ? '1px solid rgba(255,255,255,0.08)' : '1px solid rgba(24,24,27,0.08)',
      }}
    >
      <div
        style={{
          position: 'absolute',
          left: vx,
          top: vy,
          width: vw,
          height: vh,
          transform: `translate(${tx}px, ${ty}px) scale(${z.s})`,
          transformOrigin: '50% 50%',
        }}
      >
        <OffthreadVideo src={staticFile(src)} startFrom={startFrom} muted style={{width: '100%', height: '100%'}} />
      </div>
      {label ? (
        <div
          style={{
            position: 'absolute',
            right: 18,
            bottom: 16,
            padding: '6px 12px',
            borderRadius: 999,
            fontFamily: brand.font,
            fontSize: 17,
            fontWeight: 700,
            letterSpacing: 0.2,
            color: dark ? '#e4e4e7' : brand.muted,
            background: dark ? 'rgba(39,39,44,0.85)' : 'rgba(244,244,245,0.92)',
            border: dark ? '1px solid rgba(255,255,255,0.1)' : '1px solid rgba(24,24,27,0.08)',
          }}
        >
          {label}
        </div>
      ) : null}
    </div>
  );
};
