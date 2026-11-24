import {continueRender, delayRender, staticFile} from 'remotion';

let loaded = false;
export const loadFonts = () => {
  if (loaded || typeof document === 'undefined') return;
  loaded = true;
  const handle = delayRender('fonts');
  const faces = [
    new FontFace('Folio Sans', `url(${staticFile('fonts/NotoSans-Regular.ttf')})`, {weight: '400'}),
    new FontFace('Folio Sans', `url(${staticFile('fonts/NotoSans-Bold.ttf')})`, {weight: '700'}),
    new FontFace('Trailer Sans', `url(${staticFile('fonts/Inter.woff2')})`, {weight: '100 900'}),
  ];
  Promise.all(faces.map((f) => f.load()))
    .then((fs) => {
      fs.forEach((f) => document.fonts.add(f));
      continueRender(handle);
    })
    .catch(() => continueRender(handle));
};
