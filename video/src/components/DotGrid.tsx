import React from 'react';
import {AbsoluteFill, interpolate, useCurrentFrame} from 'remotion';
import {brand} from '../config/brand';
import {BEAT} from '../timeline';

// The onboarding tour's backdrop: faint dots 14 pt apart. Here they drift and breathe with the
// music: a soft pulse on every beat and a stronger one on every cut.
export const DotGrid: React.FC<{dark?: boolean; pulseAt?: number[]}> = ({dark, pulseAt = []}) => {
  const frame = useCurrentFrame();
  const beatK = Math.exp(-((frame % BEAT) / 4));
  const cutK = pulseAt.reduce((m, c) => Math.max(m, frame >= c ? Math.exp(-(frame - c) / 6) : 0), 0);
  const size = 28 + beatK * 0.8 + cutK * 3;
  const dotR = 1.6 + beatK * 0.35 + cutK * 0.9;
  const dot = dark ? `rgba(255,255,255,${0.1 + cutK * 0.1})` : brand.dot;
  const x = (frame * 0.6) % size;
  const y = (frame * 0.25) % size;
  return (
    <AbsoluteFill
      style={{
        backgroundColor: dark ? '#0b0b0c' : brand.stage,
        backgroundImage: `radial-gradient(${dot} ${dotR}px, transparent ${dotR + 0.3}px)`,
        backgroundSize: `${size}px ${size}px`,
        backgroundPosition: `${x}px ${y}px`,
        transform: `scale(${1 + cutK * 0.015})`,
      }}
    >
      <AbsoluteFill style={{opacity: interpolate(cutK, [0, 1], [0, 0.6])}} />
    </AbsoluteFill>
  );
};
