// A small, consistent icon set: 24 x 24 grid, drawn with a 1.8 px round stroke, so every
// glyph has the same weight as the text beside it. Shapes are plain path data (no markup),
// which keeps them safe to render and easy to preview outside the app.

const circle = (cx: number, cy: number, r: number): string => `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;

const rect = (x: number, y: number, w: number, h: number, r: number): string =>
  `M${x + r} ${y}h${w - 2 * r}a${r} ${r} 0 0 1 ${r} ${r}v${h - 2 * r}a${r} ${r} 0 0 1 ${-r} ${r}h${-(w - 2 * r)}a${r} ${r} 0 0 1 ${-r} ${-r}v${-(h - 2 * r)}a${r} ${r} 0 0 1 ${r} ${-r}z`;

export const ICONS = {
  search: [circle(11, 11, 6.5), 'm16 16 4.5 4.5'],
  sliders: ['M4 7h9M17 7h3M4 17h3M11 17h9', circle(15, 7, 2), circle(9, 17, 2)],
  lock: [rect(5, 10.5, 14, 9.5, 2.5), 'M8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5'],
  queue: ['M4 7h11M4 12h11M4 17h7', 'M18 14v6M15 17h6'],
  x: ['M6.5 6.5l11 11M17.5 6.5l-11 11'],
  check: ['m5 12.5 4.5 4.5L19 7.5'],
  'chevron-down': ['m6.5 9.5 5.5 5.5 5.5-5.5'],
  'chevron-up': ['m6.5 14.5 5.5-5.5 5.5 5.5'],
  'chevron-left': ['m14.5 6.5-5.5 5.5 5.5 5.5'],
  'chevron-right': ['m9.5 6.5 5.5 5.5-5.5 5.5'],
  pin: ['M12 16.5v4.5', 'M8.5 3.5h7l-1 5.5 3 3.5h-11l3-3.5-1-5.5Z'],
  edit: ['M4 20l1-4L16.5 4.5a2.1 2.1 0 0 1 3 3L8 19l-4 1Z'],
  trash: ['M5 7h14M10 7V4.8h4V7M7 7l.8 12.2h8.4L17 7M10.5 11v5M13.5 11v5'],
  eye: ['M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z', circle(12, 12, 2.8)],
  folder: ['M3.5 7.5a2 2 0 0 1 2-2H10l2 2.5h6.5a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2V7.5Z'],
  text: ['M5 7h14M5 12h14M5 17h9'],
  link: ['M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1', 'M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1'],
  code: ['m8 8-4 4 4 4M16 8l4 4-4 4M13.5 5.5l-3 13'],
  image: [rect(3.5, 5, 17, 14, 2.5), circle(9, 10, 1.5), 'm4 17 5-4.5 4 3.5 3-2.5 4.5 4'],
  file: ['M7 3.5h6.5l5 5V19a1.5 1.5 0 0 1-1.5 1.5H7A1.5 1.5 0 0 1 5.5 19V5A1.5 1.5 0 0 1 7 3.5Z', 'M13.5 3.5v5h5'],
  bolt: ['M13 3 5.5 13.5H11L10 21l7.5-10.5H12L13 3Z'],
  swatch: [circle(12, 12, 7.5)],
  clipboard: [rect(6, 5, 12, 15, 2.5), 'M9.5 5v-.5a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1V5']
} as const;

export type IconName = keyof typeof ICONS;
