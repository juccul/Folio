import React from 'react';
import {AbsoluteFill, interpolate, spring, useCurrentFrame, useVideoConfig} from 'remotion';
import {brand} from '../config/brand';
import {content} from '../config/content';
import {Icon} from './Icon';
import {Format} from './layout';

// Asymmetric closing grid: one idea per tile, the largest tile for the biggest promise.
const grids: Record<Format, {cols: string; rows: string; areas: string[]; pad: number[]; gap: number; big: number; small: number}> = {
  '16:9': {
    cols: '1.35fr 1fr 1fr',
    rows: '1fr 1fr 1fr',
    areas: ['1 / 1 / 3 / 2', '1 / 2 / 2 / 3', '1 / 3 / 2 / 4', '2 / 2 / 3 / 4', '3 / 1 / 4 / 2', '3 / 2 / 4 / 3', '3 / 3 / 4 / 4'],
    pad: [130, 150],
    gap: 22,
    big: 64,
    small: 34,
  },
  '9:16': {
    cols: '1fr 1fr',
    rows: '1.5fr 1fr 1fr 1fr',
    areas: ['1 / 1 / 2 / 3', '2 / 1 / 3 / 2', '2 / 2 / 3 / 3', '3 / 1 / 4 / 3', '4 / 1 / 5 / 2', '4 / 2 / 5 / 3'],
    pad: [270, 70],
    gap: 18,
    big: 60,
    small: 32,
  },
  '1:1': {
    cols: '1.3fr 1fr',
    rows: '1fr 1fr 1fr',
    areas: ['1 / 1 / 3 / 2', '1 / 2 / 2 / 3', '2 / 2 / 3 / 3', '3 / 1 / 4 / 2', '3 / 2 / 4 / 3'],
    pad: [80, 80],
    gap: 18,
    big: 58,
    small: 32,
  },
};

export const BentoSummary: React.FC<{format: Format; duration: number}> = ({format, duration}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const g = grids[format];
  const tiles = content.bento.slice(0, g.areas.length);
  const exit = interpolate(frame, [duration - 10, duration], [0, 1], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});
  // 9:16 stays inside the universal safe core (X 60..900, Y 260..1280).
  const padding = format === '9:16' ? '270px 180px 640px 60px' : `${g.pad[0]}px ${g.pad[1]}px`;
  return (
    <AbsoluteFill
      style={{
        padding,
        display: 'grid',
        gridTemplateColumns: g.cols,
        gridTemplateRows: g.rows,
        gap: g.gap,
              }}
    >
      {tiles.map((tile, i) => {
        // Tiles land one per half-beat with an overshoot, from alternating sides.
        const k = spring({frame: frame - 4 - i * 4, fps, config: {damping: 12, stiffness: 170, mass: 0.7}});
        const side = i % 2 ? 1 : -1;
        const iconPop = spring({frame: frame - 12 - i * 4, fps, config: {damping: 9, stiffness: 200}});
        const hero = i === 0;
        return (
          <div
            key={tile.title}
            style={{
              gridArea: g.areas[i],
              borderRadius: brand.radius,
              background: hero ? `linear-gradient(180deg, ${brand.tileTop}, ${brand.tileBottom})` : brand.paper,
              boxShadow: brand.shadow,
              padding: hero ? 44 : 30,
              display: 'flex',
              flexDirection: 'column',
              justifyContent: 'space-between',
              opacity: Math.min(1, k * 1.5) * (1 - exit),
              transform: `translate(${(1 - k) * side * 160}px, ${(1 - k) * 120}px) rotate(${(1 - k) * side * 8}deg) scale(${(0.6 + 0.4 * k) * (1 + exit * 0.08)})`,
            }}
          >
            <div style={{transform: `scale(${iconPop}) rotate(${(1 - iconPop) * -40}deg)`, transformOrigin: '0 50%'}}>
              <Icon name={tile.icon} size={hero ? 56 : 40} color={hero ? brand.answer : brand.ink} />
            </div>
            <div>
              <div
                style={{
                  fontFamily: brand.font,
                  fontWeight: 700,
                  fontSize: hero ? g.big : g.small,
                  letterSpacing: -0.6,
                  lineHeight: 1.08,
                  color: hero ? brand.nib : brand.ink,
                }}
              >
                {tile.title}
              </div>
              {hero && tile.sub ? (
                <div style={{fontFamily: brand.font, fontSize: g.small * 0.8, color: '#a1a1aa', marginTop: 14, lineHeight: 1.3}}>
                  {tile.sub}
                </div>
              ) : null}
            </div>
          </div>
        );
      })}
    </AbsoluteFill>
  );
};
