import React from 'react';
import {Easing, interpolate, spring, useCurrentFrame, useVideoConfig} from 'remotion';
import {brand, ease} from '../config/brand';

type Props = {
  text: string;
  accent?: string; // a word marked with the app's highlighter and drawn in amber ink
  size: number;
  start?: number;
  exitAt?: number; // local frame the words start leaving
  stagger?: number;
  color?: string;
  weight?: number;
  dark?: boolean;
};

// Words spring up from behind hard masks with a little overshoot; the accent word gets a
// highlighter swipe like Folio's search highlight; on exit the words snap upward.
export const MaskedHeadline: React.FC<Props> = ({
  text,
  accent,
  size,
  start = 0,
  exitAt,
  stagger = 3,
  color = brand.ink,
  weight = 700,
  dark,
}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const words = text.split(' ');
  return (
    <div style={{display: 'flex', flexWrap: 'wrap', columnGap: size * 0.26, rowGap: size * 0.02}}>
      {words.map((word, i) => {
        const t0 = start + i * stagger;
        const inK = spring({frame: frame - t0, fps, config: {damping: 13, stiffness: 190, mass: 0.7}});
        const outK =
          exitAt === undefined
            ? 0
            : interpolate(frame, [exitAt + i, exitAt + i + 7], [0, 1], {
                extrapolateLeft: 'clamp',
                extrapolateRight: 'clamp',
                easing: Easing.in(Easing.cubic),
              });
        const y = (1 - inK) * 115 - outK * 120;
        const rot = (1 - inK) * 8;
        const isAccent = !!accent && word.replace(/[.,]/g, '') === accent.replace(/[.,]/g, '');
        const mark = isAccent
          ? interpolate(frame, [t0 + 10, t0 + 20], [0, 1], {
              extrapolateLeft: 'clamp',
              extrapolateRight: 'clamp',
              easing: Easing.bezier(...ease.snap),
            })
          : 0;
        return (
          <span
            key={i}
            style={{
              position: 'relative',
              display: 'inline-block',
              overflow: 'hidden',
              lineHeight: 1.12,
              padding: `0 ${size * 0.06}px ${size * 0.1}px`,
              margin: `0 ${-size * 0.06}px ${-size * 0.1}px`,
            }}
          >
            {isAccent ? (
              <span
                style={{
                  position: 'absolute',
                  left: size * 0.02,
                  right: size * 0.02,
                  top: size * 0.52,
                  height: size * 0.5,
                  borderRadius: size * 0.08,
                  background: dark ? 'rgba(217,162,27,0.28)' : brand.highlight,
                  transform: `scaleX(${mark * (1 - outK)}) skewX(-8deg)`,
                  transformOrigin: '0 50%',
                }}
              />
            ) : null}
            <span
              style={{
                position: 'relative',
                display: 'inline-block',
                transform: `translateY(${y}%) rotate(${rot}deg)`,
                transformOrigin: '0 100%',
                fontFamily: brand.font,
                fontWeight: weight,
                fontSize: size,
                letterSpacing: -size * 0.03,
                color: isAccent ? (dark ? brand.answer : '#b7800c') : color,
              }}
            >
              {word}
            </span>
          </span>
        );
      })}
    </div>
  );
};
