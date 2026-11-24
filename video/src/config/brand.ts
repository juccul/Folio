// Tokens taken from the Folio repo: packaging/io.github.folio.Notes.svg (icon tile),
// crates/app/src/appearance.rs (neutral zinc theme), crates/ui/src/frame.rs (Noto Sans),
// and colours sampled from the app's own recorded ink (auto-math answer, search highlight).
export const brand = {
  font: 'Folio Sans',
  ink: '#0d0d0d',
  paper: '#ffffff',
  stage: '#f4f4f5',
  dot: '#d4d4d8',
  rule: '#e4e4e7',
  muted: '#71717a',
  tileTop: '#27272c',
  tileBottom: '#141417',
  nib: '#f4f4f6',
  darkPaper: '#0e0e0e',
  answer: '#d9a21b',
  highlight: '#ffdbba',
  highlightEdge: '#f08a24',
  radius: 22,
  shadow: '0 2px 6px rgba(0,0,0,0.06), 0 24px 60px rgba(24,24,27,0.16)',
};

export const ease = {
  // cubic-bezier(0.7, 0, 0.2, 1)
  snap: [0.7, 0, 0.2, 1] as const,
  out: [0.16, 1, 0.3, 1] as const,
};
