// Footage recorded headlessly from the real app.
//   clips/tour/*: the app's own onboarding clips (scripts/record-tour.py), 960x600.
//   clips/own/*:  full-window captures made for this video (capture/scenes.py), 1280x800 pt at 2x.
export type Clip = {src: string; frames: number; aspect: number; dark?: boolean};
export const clips: Record<string, Clip> = {
  math: {src: 'clips/tour/math-light.mp4', frames: 162, aspect: 1.6},
  snap: {src: 'clips/tour/snap-light.mp4', frames: 147, aspect: 1.6},
  encircle: {src: 'clips/tour/encircle-light.mp4', frames: 158, aspect: 1.6},
  scratch: {src: 'clips/tour/scratch-light.mp4', frames: 111, aspect: 1.6},
  convert: {src: 'clips/tour/convert-light.mp4', frames: 198, aspect: 1.6},
  search: {src: 'clips/tour/search-light.mp4', frames: 143, aspect: 1.6},
  editor: {src: 'clips/own/editor.mp4', frames: 431, aspect: 1.6},
  library: {src: 'clips/own/library.mp4', frames: 251, aspect: 1.6},
  solveDark: {src: 'clips/own/dark.mp4', frames: 277, aspect: 1.6, dark: true},
};
