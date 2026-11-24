import React from 'react';
import {staticFile} from 'remotion';

// Folio's own Tabler icons, tinted with a CSS mask.
export const Icon: React.FC<{name: string; size: number; color: string}> = ({name, size, color}) => (
  <div
    style={{
      width: size,
      height: size,
      backgroundColor: color,
      WebkitMaskImage: `url(${staticFile(`icons/${name}.svg`)})`,
      WebkitMaskSize: 'contain',
      WebkitMaskRepeat: 'no-repeat',
      maskImage: `url(${staticFile(`icons/${name}.svg`)})`,
      maskSize: 'contain',
      maskRepeat: 'no-repeat',
    }}
  />
);
