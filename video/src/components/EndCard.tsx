import React from 'react';
import {AbsoluteFill, Easing, Img, interpolate, spring, staticFile, useCurrentFrame, useVideoConfig} from 'remotion';
import {brand, ease} from '../config/brand';
import {content} from '../config/content';
import {MaskedHeadline} from './MaskedHeadline';
import {BEAT} from '../timeline';
import {Format} from './layout';

// Brief end-loaded lockup: icon, name, one action, one URL.
export const EndCard: React.FC<{format: Format}> = ({format}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const pop = spring({frame, fps, config: {damping: 18, stiffness: 220}});
  const k = (a: number) =>
    interpolate(frame, [a, a + 14], [0, 1], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp', easing: Easing.bezier(...ease.out)});
  const vertical = format === '9:16';
  const icon = format === '16:9' ? 150 : 140;
  return (
    <AbsoluteFill
      style={{
        alignItems: 'center',
        justifyContent: 'center',
        flexDirection: 'column',
        gap: 26,
        // 9:16: centre the lockup inside the safe core rather than the frame.
        paddingBottom: vertical ? 1920 - 1280 - 260 : 0,
        paddingRight: vertical ? 120 : 0,
      }}
    >
      <div style={{position: 'relative', width: icon, height: icon}}>
        <Img src={staticFile('brand/folio.svg')} style={{width: icon, height: icon, transform: `scale(${pop}) rotate(${(1 - pop) * -20}deg)`}} />
        {/* Encircled, the way you select ink in Folio. */}
        <svg width={icon * 2} height={icon * 1.7} style={{position: 'absolute', left: -icon * 0.5, top: -icon * 0.35, overflow: 'visible'}}>
          <path
            d={`M ${icon * 1.05} ${icon * 0.08} C ${icon * 1.75} ${icon * 0.12}, ${icon * 1.95} ${icon * 0.95}, ${icon * 1.45} ${icon * 1.42} C ${icon * 1.0} ${icon * 1.8}, ${icon * 0.2} ${icon * 1.6}, ${icon * 0.08} ${icon * 0.95} C ${-icon * 0.02} ${icon * 0.35}, ${icon * 0.55} ${icon * 0.02}, ${icon * 1.2} ${icon * 0.16}`}
            fill="none"
            stroke={brand.answer}
            strokeWidth={7}
            strokeLinecap="round"
            pathLength={1}
            strokeDasharray={1}
            strokeDashoffset={1 - interpolate(frame, [6, 22], [0, 1], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp', easing: Easing.inOut(Easing.cubic)})}
          />
        </svg>
      </div>
      <MaskedHeadline text={content.end.name} size={format === '16:9' ? 104 : 96} start={4} stagger={0} />
      <div
        style={{
          fontFamily: brand.font,
          fontSize: format === '16:9' ? 34 : 32,
          color: brand.muted,
          opacity: k(12),
          textAlign: 'center',
          maxWidth: 820,
        }}
      >
        {content.end.line}
      </div>
      <div
        style={{
          marginTop: 10,
          display: 'flex',
          flexDirection: vertical ? 'column' : 'row',
          alignItems: 'center',
          gap: 18,
          opacity: k(20),
          transform: `translateY(${(1 - k(20)) * 16}px)`,
        }}
      >
        <div
          style={{
            fontFamily: brand.font,
            fontWeight: 700,
            fontSize: 32,
            color: brand.nib,
            background: `linear-gradient(180deg, ${brand.tileTop}, ${brand.tileBottom})`,
            padding: '18px 34px',
            borderRadius: 999,
            transform: `scale(${1 + 0.05 * Math.exp(-((frame % BEAT) / 3)) * (frame > 30 ? 1 : 0)})`,
          }}
        >
          {content.end.cta}
        </div>
        <div style={{fontFamily: brand.font, fontSize: 30, color: brand.ink, fontWeight: 700}}>{content.end.url}</div>
      </div>
    </AbsoluteFill>
  );
};
