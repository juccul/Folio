import {BeatContent, content} from './config/content';
import timing from './config/timing.json';

export const FPS = timing.fps;
export const OVERLAP = timing.overlap;
export const BEAT = (60 / timing.bpm) * FPS; // frames per beat

type Cut = keyof Pick<typeof timing, 'flagship' | 'teaser'>;

export type Placed = BeatContent & {from: number; duration: number; index: number};

// Every scene starts on a beat of the music; each overlaps the next by OVERLAP frames for the whip.
export const place = (cut: Cut) => {
  const t = timing[cut];
  const slots = t.slots as Record<string, number>;
  let at = 0;
  const placed: Placed[] = Object.entries(slots).map(([key, slot], index) => {
    const beat = content.beats.find((b) => b.key === key)!;
    const p = {...beat, sub: cut === 'teaser' ? undefined : beat.sub, from: at, duration: slot + OVERLAP, index};
    at += slot;
    return p;
  });
  const bentoFrom = t.bento ? at : -1;
  at += t.bento;
  return {placed, bentoFrom, bento: t.bento, endFrom: at, total: at + t.end, cuts: placed.map((p) => p.from).slice(1)};
};

export const flagship = () => place('flagship');
export const teaser = () => place('teaser');
