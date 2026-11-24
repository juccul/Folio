// Every word on screen lives here. Beats reference clips by key (see clips.ts).
// Rule Zero
//   Positioning: helps people who write with a pen on Linux or Windows see that Folio turns
//   handwriting into notes that select, erase, convert, solve and search — offline — so they download it.
//   Colleague sentence: "Folio is an offline notes app where your handwriting does the math,
//   turns into text and is searchable — no account, no cloud."
//   Single visual proof: "12+30=" written by hand, and Folio writes "42" in the same hand.

export type BeatContent = {
  key: string;
  clip: string;
  headline: string;
  accent?: string;
  sub?: string;
  label?: string;
  underline?: boolean;
  trimStart?: number; // frames dropped from the clip's head
  zooms?: {f: number; x: number; y: number; s: number}[];
  zoomsV?: {f: number; x: number; y: number; s: number}[]; // 9:16 and 1:1
  dark?: boolean;
};

export const REAL = 'Real app · 1.0× speed';

export const content = {
  beats: [
    {
      key: 'hook',
      clip: 'math',
      headline: 'Your handwriting does the math.',
      accent: 'math.',
      label: REAL,
      trimStart: 12,
      zooms: [{f: 0, x: 0.5, y: 0.5, s: 1.12}, {f: 52, x: 0.5, y: 0.5, s: 1.12}, {f: 74, x: 0.6, y: 0.46, s: 1.32}],
    },
    {
      key: 'context',
      clip: 'editor',
      headline: 'Offline notes for your pen.',
      sub: 'Folio for Linux and Windows. No account, no cloud.',
      label: REAL,
      trimStart: 15,
      zooms: [{f: 0, x: 0.62, y: 0.4, s: 1.25}, {f: 48, x: 0.62, y: 0.4, s: 1.25}, {f: 68, x: 0.42, y: 0.3, s: 1.7}],
      zoomsV: [{f: 0, x: 0.75, y: 0.4, s: 1.6}, {f: 48, x: 0.75, y: 0.4, s: 1.6}, {f: 68, x: 0.4, y: 0.3, s: 2.1}],
    },
    {
      key: 'snap',
      clip: 'snap',
      headline: 'Hold a stroke to perfect it.',
      accent: 'perfect',
      label: REAL,
      trimStart: 12,
      zooms: [{f: 0, x: 0.5, y: 0.5, s: 1}, {f: 20, x: 0.42, y: 0.42, s: 1.2}, {f: 80, x: 0.42, y: 0.42, s: 1.2}, {f: 100, x: 0.6, y: 0.5, s: 1.12}],
    },
    {
      key: 'select',
      clip: 'encircle',
      headline: 'Circle ink to select it.',
      accent: 'Circle',
      label: REAL,
      trimStart: 6,
      zooms: [{f: 0, x: 0.5, y: 0.5, s: 1}, {f: 24, x: 0.5, y: 0.58, s: 1.18}],
    },
    {
      key: 'erase',
      clip: 'scratch',
      headline: 'Scribble it out.',
      accent: 'Scribble',
      label: REAL,
      trimStart: 6,
      zooms: [{f: 0, x: 0.5, y: 0.5, s: 1.05}, {f: 20, x: 0.45, y: 0.5, s: 1.28}],
    },
    {
      key: 'convert',
      clip: 'convert',
      headline: 'Turn writing into text.',
      accent: 'text.',
      label: REAL,
      trimStart: 15,
      zooms: [{f: 0, x: 0.5, y: 0.45, s: 1.15}, {f: 40, x: 0.5, y: 0.5, s: 1}],
    },
    {
      key: 'solve',
      clip: 'solveDark',
      headline: 'Solve it, step by step.',
      accent: 'Solve',
      sub: 'Offline solver. Shown in dark theme.',
      label: REAL,
      dark: true,
      trimStart: 40,
      zooms: [{f: 0, x: 0.3, y: 0.3, s: 1.5}, {f: 40, x: 0.3, y: 0.3, s: 1.5}, {f: 62, x: 0.62, y: 0.4, s: 1.3}, {f: 110, x: 0.82, y: 0.42, s: 1.7}],
      zoomsV: [{f: 0, x: 0.3, y: 0.3, s: 1.9}, {f: 44, x: 0.3, y: 0.3, s: 1.9}, {f: 70, x: 0.82, y: 0.4, s: 2.0}],
    },
    {
      key: 'search',
      clip: 'search',
      headline: 'Search your handwriting.',
      accent: 'Search',
      label: REAL,
      trimStart: 6,
      zooms: [{f: 0, x: 0.5, y: 0.5, s: 1}, {f: 50, x: 0.58, y: 0.45, s: 1.18}],
    },
    {
      key: 'library',
      clip: 'library',
      headline: 'Every notebook, one shelf.',
      accent: 'one',
      sub: 'Ctrl+T jumps to any document.',
      label: REAL,
      trimStart: 12,
      zooms: [{f: 0, x: 0.55, y: 0.35, s: 1.3}, {f: 60, x: 0.55, y: 0.35, s: 1.3}, {f: 82, x: 0.6, y: 0.36, s: 1.55}],
      zoomsV: [{f: 0, x: 0.62, y: 0.32, s: 1.6}, {f: 60, x: 0.62, y: 0.32, s: 1.6}, {f: 82, x: 0.6, y: 0.36, s: 1.95}],
    },
  ] as BeatContent[],
  bento: [
    {icon: 'Pen', title: 'Runs fully offline', sub: 'Recognition and solving on your CPU. Nothing is uploaded.'},
    {icon: 'Library', title: 'No account'},
    {icon: 'Page', title: 'Annotate PDFs'},
    {icon: 'Export', title: 'Export PDF, SVG, PNG'},
    {icon: 'Palette', title: 'Linux & Windows'},
    {icon: 'TextScan', title: 'Open source, GPL-3.0'},
    {icon: 'Search', title: 'Search ink'},
  ],
  end: {
    name: 'Folio',
    line: 'A notebook that understands your handwriting.',
    cta: 'Download free',
    url: 'github.com/juccul/Folio',
  },
};
