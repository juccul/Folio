import React from 'react';
import {AbsoluteFill, Easing, interpolate, useCurrentFrame, useVideoConfig} from 'remotion';

// A huge pen stroke zig-zags across the frame, covering the cut, then lifts off the other end.
export const InkWipe: React.FC<{at: number; color: string; dur?: number}> = ({at, color, dur = 18}) => {
  const frame = useCurrentFrame();
  const {width, height} = useVideoConfig();
  const half = dur / 2;
  const draw = interpolate(frame, [at - half, at], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.in(Easing.quad),
  });
  const lift = interpolate(frame, [at, at + half], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.quad),
  });
  if (draw <= 0 || lift >= 1) return null;
  const sw = Math.max(width, height) * 0.42;
  const pad = sw * 0.6;
  // Three diagonal passes, like a scratch-out, wide enough to cover the frame.
  const d = `M ${-pad} ${height * 0.1} L ${width + pad} ${-pad * 0.4} L ${-pad} ${height * 0.62} L ${width + pad} ${height * 0.35} L ${-pad} ${height + pad * 0.5} L ${width + pad} ${height + pad * 0.2}`;
  const len = (width + 2 * pad) * 3.4 + height * 2;
  const shown = draw - lift;
  return (
    <AbsoluteFill style={{pointerEvents: 'none'}}>
      <svg width={width} height={height} style={{overflow: 'visible'}}>
        <path
          d={d}
          fill="none"
          stroke={color}
          strokeWidth={sw}
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeDasharray={`${len * shown} ${len * 2}`}
          strokeDashoffset={-len * lift}
        />
      </svg>
    </AbsoluteFill>
  );
};
