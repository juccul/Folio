import React from 'react';
import {interpolate, useCurrentFrame} from 'remotion';
import {brand} from '../config/brand';

// A pen stroke drawn under the headline, like ink laid down on the page.
export const InkUnderline: React.FC<{width: number; start: number; color?: string}> = ({
  width,
  start,
  color = brand.answer,
}) => {
  const frame = useCurrentFrame();
  const k = interpolate(frame, [start, start + 14], [0, 1], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});
  const len = width * 1.08;
  const d = `M4 14 C ${width * 0.25} 6, ${width * 0.55} 18, ${width * 0.8} 9 S ${width - 6} 10, ${width - 2} 8`;
  return (
    <svg width={width} height={24} style={{display: 'block', overflow: 'visible'}}>
      <path
        d={d}
        fill="none"
        stroke={color}
        strokeWidth={6}
        strokeLinecap="round"
        strokeDasharray={len}
        strokeDashoffset={len * (1 - k)}
      />
    </svg>
  );
};
