import {Easing, interpolate} from 'remotion';
import {Format} from '../components/layout';

export const T = {
  font: 'Trailer Sans',
  black: '#000000',
  white: '#f5f5f7',
  gray: '#86868b',
  silver: 'linear-gradient(180deg, #ffffff 0%, #d4d4d8 55%, #9a9aa0 100%)',
  warm: 'linear-gradient(90deg, #f9dc8c 0%, #f2b23a 45%, #ea7c2c 100%)',
};

// Text that sharpens out of a blur, settling its tracking as it lands.
export const reveal = (frame: number, start: number, dur = 18) => {
  const k = interpolate(frame, [start, start + dur], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.bezier(0.16, 1, 0.3, 1),
  });
  return {
    opacity: k,
    filter: k < 1 ? `blur(${(1 - k) * 18}px)` : undefined,
    transform: `translateY(${(1 - k) * 26}px) scale(${1.04 - 0.04 * k})`,
    letterSpacing: `${(1 - k) * 0.05 - 0.015}em`,
  };
};

// Fades a scene's content out over its last frames (cuts to black).
export const tail = (frame: number, len: number, n = 8) =>
  interpolate(frame, [len - n, len], [1, 0], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});

// Headline text: solid near-white, Semibold, tight tracking (apple.com hero spec: 600, lh 1.05, -0.015em).
export const head = (size: number): React.CSSProperties => ({
  fontFamily: T.font,
  fontWeight: 600,
  fontSize: size,
  lineHeight: 1.05,
  color: T.white,
});

export const caption = (size: number): React.CSSProperties => ({
  fontFamily: T.font,
  fontWeight: 400,
  fontSize: Math.max(32, size),
  lineHeight: 1.25,
  color: T.gray,
  letterSpacing: '0.007em',
});

export const gradientText = (bg: string): React.CSSProperties => ({
  backgroundImage: bg,
  WebkitBackgroundClip: 'text',
  backgroundClip: 'text',
  color: 'transparent',
});

// Per-format staging. 9:16 keeps text and product inside the universal safe core (X 60..900, Y 260..1280).
export const stage = (format: Format) => {
  if (format === '16:9')
    return {w: 1920, h: 1080, cx: 960, cy: 540, s: 1, deviceW: 1060, deviceY: 610, titleY: 150, splitDeviceW: 980, vertical: false};
  if (format === '9:16')
    return {w: 1080, h: 1920, cx: 480, cy: 770, s: 0.8, deviceW: 820, deviceY: 900, titleY: 300, splitDeviceW: 820, vertical: true};
  return {w: 1080, h: 1080, cx: 540, cy: 540, s: 0.72, deviceW: 820, deviceY: 620, titleY: 70, splitDeviceW: 760, vertical: true};
};
