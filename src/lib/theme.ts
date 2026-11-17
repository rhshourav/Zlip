import type { UiConfig } from './types';

/** Turn the config's theme section into CSS variables and data attributes. */
export function applyTheme(cfg: UiConfig, root: HTMLElement = document.documentElement): void {
  const { theme, os } = cfg;
  for (const [name, hex] of Object.entries(theme.categories)) {
    root.style.setProperty(`--cat-${name}`, hex);
  }
  root.style.setProperty('--tint', String(theme.tint_opacity));
  root.style.setProperty('--zlip-radius', `${theme.corner_radius}px`);
  if (theme.mode === 'auto') delete root.dataset.theme;
  else root.dataset.theme = theme.mode;
  root.dataset.motion = theme.animations ? 'on' : 'off';
  // Windows 11 rounds window corners; on Windows 10 the panel stays square.
  root.dataset.rounded = os.rounded ? 'yes' : 'no';
  // Without Transparency effects there is no backdrop to show through.
  root.dataset.glass = os.transparency ? 'on' : 'off';
}
