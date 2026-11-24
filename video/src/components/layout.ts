export type Format = '16:9' | '9:16' | '1:1';

export const dims: Record<Format, {w: number; h: number}> = {
  '16:9': {w: 1920, h: 1080},
  '9:16': {w: 1080, h: 1920},
  '1:1': {w: 1080, h: 1080},
};

// Where the headline column and the product card sit in each format.
// 9:16 keeps everything inside the universal safe core (X 60..900, Y 260..1280).
export const layout = (format: Format) => {
  if (format === '16:9') {
    return {
      head: {left: 110, top: 0, width: 620, height: 1080, align: 'center' as const},
      headSize: 84,
      subSize: 28,
      card: {left: 730, top: 202, width: 1088, height: 680},
    };
  }
  if (format === '9:16') {
    return {
      head: {left: 66, top: 270, width: 828, height: 380, align: 'end' as const},
      headSize: 80,
      subSize: 30,
      card: {left: 60, top: 700, width: 840, height: 525},
    };
  }
  return {
    head: {left: 80, top: 60, width: 920, height: 250, align: 'end' as const},
    headSize: 72,
    subSize: 28,
    card: {left: 110, top: 360, width: 860, height: 538},
  };
};
