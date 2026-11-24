import React from 'react';
import {Composition} from 'remotion';
import {HeroLoop, LaunchVideo, heroLength} from './LaunchVideo';
import {dims} from './components/layout';
import {FPS, flagship, teaser} from './timeline';
import {Trailer, trailerLength} from './trailer/Trailer';
import {SongTrailer, songTrailerLength} from './song/SongTrailer';

export const Root: React.FC = () => {
  const full = flagship().total;
  const short = teaser().total;
  const tf = trailerLength('flagship');
  const tt = trailerLength('teaser');
  return (
    <>
      {/* Cut to the song (Adam F, "Circles Revisited", 165 BPM): drop on bar 16. */}
      <Composition id="Song16x9" component={SongTrailer} durationInFrames={songTrailerLength()} fps={FPS} width={1920} height={1080} defaultProps={{format: '16:9' as const, cut: 'full' as const}} />
      <Composition id="Song9x16" component={SongTrailer} durationInFrames={songTrailerLength()} fps={FPS} width={1080} height={1920} defaultProps={{format: '9:16' as const, cut: 'full' as const}} />
      <Composition id="Song1x1" component={SongTrailer} durationInFrames={songTrailerLength()} fps={FPS} width={1080} height={1080} defaultProps={{format: '1:1' as const, cut: 'full' as const}} />
      <Composition id="SongTeaser9x16" component={SongTrailer} durationInFrames={songTrailerLength('teaser')} fps={FPS} width={1080} height={1920} defaultProps={{format: '9:16' as const, cut: 'teaser' as const}} />
      {/* Product-trailer cut (black stage, floating device). */}
      <Composition id="Trailer16x9" component={Trailer} durationInFrames={tf} fps={FPS} width={1920} height={1080} defaultProps={{format: '16:9' as const, cut: 'flagship' as const}} />
      <Composition id="Trailer9x16" component={Trailer} durationInFrames={tf} fps={FPS} width={1080} height={1920} defaultProps={{format: '9:16' as const, cut: 'flagship' as const}} />
      <Composition id="Trailer1x1" component={Trailer} durationInFrames={tf} fps={FPS} width={1080} height={1080} defaultProps={{format: '1:1' as const, cut: 'flagship' as const}} />
      <Composition id="TrailerTeaser9x16" component={Trailer} durationInFrames={tt} fps={FPS} width={1080} height={1920} defaultProps={{format: '9:16' as const, cut: 'teaser' as const}} />
      {/* Earlier paper-stage cut. */}
      <Composition id="Launch16x9" component={LaunchVideo} durationInFrames={full} fps={FPS} width={dims['16:9'].w} height={dims['16:9'].h} defaultProps={{format: '16:9' as const, cut: 'flagship' as const, music: 'audio/bed-flagship.wav'}} />
      <Composition id="Launch9x16" component={LaunchVideo} durationInFrames={full} fps={FPS} width={dims['9:16'].w} height={dims['9:16'].h} defaultProps={{format: '9:16' as const, cut: 'flagship' as const, music: 'audio/bed-flagship.wav'}} />
      <Composition id="Launch1x1" component={LaunchVideo} durationInFrames={full} fps={FPS} width={dims['1:1'].w} height={dims['1:1'].h} defaultProps={{format: '1:1' as const, cut: 'flagship' as const, music: 'audio/bed-flagship.wav'}} />
      <Composition id="Teaser9x16" component={LaunchVideo} durationInFrames={short} fps={FPS} width={dims['9:16'].w} height={dims['9:16'].h} defaultProps={{format: '9:16' as const, cut: 'teaser' as const, music: 'audio/bed-teaser.wav'}} />
      <Composition id="HeroLoop" component={HeroLoop} durationInFrames={heroLength()} fps={FPS} width={1280} height={720} />
    </>
  );
};
