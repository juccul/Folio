import React from 'react';
import {AbsoluteFill, Easing, interpolate, spring, useCurrentFrame, useVideoConfig} from 'remotion';
import {brand} from '../config/brand';
import {OVERLAP} from '../timeline';
import {ClipCard, Zoom} from './ClipCard';
import {InkUnderline} from './InkUnderline';
import {MaskedHeadline} from './MaskedHeadline';
import {Format, layout} from './layout';

export type BeatProps = {
  format: Format;
  duration: number;
  headline: string;
  accent?: string;
  sub?: string;
  clip: {src: string; aspect: number; dark?: boolean};
  zooms?: Zoom[];
  startFrom?: number;
  label?: string;
  underline?: boolean;
  dark?: boolean;
  first?: boolean;
  step?: string;
};

// One step of the workflow: a punchy headline beside the real app doing it.
// The card springs in tilted, drifts while it plays, and whips out with motion blur on the next beat.
export const Beat: React.FC<BeatProps> = ({
  format,
  duration,
  headline,
  accent,
  sub,
  clip,
  zooms,
  startFrom,
  label,
  underline,
  dark,
  first,
  step,
}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const L = layout(format);
  const vertical = format !== '16:9';
  const enter = first ? 1 : spring({frame, fps, config: {damping: 14, stiffness: 150, mass: 0.8}});
  const whipStart = duration - OVERLAP - 4;
  const whip = interpolate(frame, [whipStart, duration], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.in(Easing.cubic),
  });
  const span = vertical ? 1400 : 2000;
  const drift = interpolate(frame, [0, duration], [0, 1]);
  const enterSpeed = first ? 0 : Math.abs(spring({frame: frame - 1, fps, config: {damping: 14, stiffness: 150, mass: 0.8}}) - enter);
  const tx = vertical ? 0 : (1 - enter) * 900 - whip * span - drift * 24;
  const ty = vertical ? (1 - enter) * 700 - whip * span : 0;
  const rotY = vertical ? 0 : (1 - enter) * -28 + whip * 18;
  const rotX = vertical ? (1 - enter) * 24 - whip * 14 : 0;
  const rotZ = (1 - enter) * (vertical ? 0 : 4) - whip * 3 + Math.sin(frame / 22) * 0.35;
  const scale = (0.82 + 0.18 * enter) * (1 + drift * 0.035) * (1 - whip * 0.12);
  const blur = Math.min(18, whip * 22 + enterSpeed * 160);
  const subK = interpolate(frame, [14, 24], [0, 1], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});
  const subOut = interpolate(frame, [whipStart, whipStart + 6], [0, 1], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});
  const textColor = dark ? '#fafafa' : brand.ink;
  const stepK = spring({frame: frame - 2, fps, config: {damping: 16, stiffness: 220}});
  return (
    <AbsoluteFill style={{perspective: 1800}}>
      <div
        style={{
          position: 'absolute',
          ...L.head,
          display: 'flex',
          flexDirection: 'column',
          justifyContent: L.head.align === 'center' ? 'center' : 'flex-end',
          gap: 18,
        }}
      >
        {step ? (
          <div
            style={{
              alignSelf: 'flex-start',
              fontFamily: brand.font,
              fontWeight: 700,
              fontSize: L.subSize * 0.8,
              letterSpacing: 1.5,
              color: dark ? '#0d0d0d' : brand.nib,
              background: dark ? brand.answer : brand.tileTop,
              borderRadius: 999,
              padding: '6px 14px',
              transform: `scale(${stepK * (1 - subOut)})`,
              transformOrigin: '0 50%',
            }}
          >
            {step}
          </div>
        ) : null}
        <MaskedHeadline
          text={headline}
          accent={accent}
          size={L.headSize}
          start={first ? 0 : 3}
          exitAt={whipStart}
          color={textColor}
          dark={dark}
        />
        {underline ? (
          <div style={{opacity: 1 - subOut}}>
            <InkUnderline width={L.headSize * 3.4} start={18} />
          </div>
        ) : null}
        {sub ? (
          <div
            style={{
              fontFamily: brand.font,
              fontSize: L.subSize,
              lineHeight: 1.35,
              color: dark ? '#a1a1aa' : brand.muted,
              opacity: subK * (1 - subOut),
              transform: `translateY(${(1 - subK) * 14 - subOut * 20}px)`,
            }}
          >
            {sub}
          </div>
        ) : null}
      </div>
      <div
        style={{
          position: 'absolute',
          left: L.card.left,
          top: L.card.top,
          transform: `translate(${tx}px, ${ty}px) rotateY(${rotY}deg) rotateX(${rotX}deg) rotateZ(${rotZ}deg) scale(${scale})`,
          transformOrigin: '50% 50%',
          filter: blur > 0.3 ? `blur(${blur}px)` : undefined,
          opacity: first ? 1 : Math.min(1, enter * 2),
        }}
      >
        <ClipCard
          src={clip.src}
          aspect={clip.aspect}
          width={L.card.width}
          height={L.card.height}
          dark={clip.dark}
          zooms={zooms}
          startFrom={startFrom}
          label={label}
        />
      </div>
    </AbsoluteFill>
  );
};
