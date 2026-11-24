import React from 'react';
import {AbsoluteFill, interpolate, useCurrentFrame} from 'remotion';
import {brand} from '../config/brand';

// The tour's water: a ring rolls out across the dot grid from the product at every cut.
export const Ripple: React.FC<{x: number; y: number; at: number[]; dark?: boolean}> = ({x, y, at, dark}) => {
  const frame = useCurrentFrame();
  return (
    <AbsoluteFill style={{pointerEvents: 'none'}}>
      {at.map((t) => {
        const k = interpolate(frame, [t, t + 36], [0, 1], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});
        if (k <= 0 || k >= 1) return null;
        const r = 120 + k * 1500;
        const o = (1 - k) * 0.55;
        return (
          <div
            key={t}
            style={{
              position: 'absolute',
              left: x - r,
              top: y - r,
              width: r * 2,
              height: r * 2,
              borderRadius: '50%',
              border: `${10 * (1 - k) + 2}px solid ${dark ? 'rgba(217,162,27,0.5)' : brand.answer}`,
              opacity: o,
            }}
          />
        );
      })}
    </AbsoluteFill>
  );
};
