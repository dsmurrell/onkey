// The design tokens of app/globals.css as hex strings, for canvas and SVG code that can't read a CSS var.
// It mirrors @theme there (lib/colors.test.mjs checks the two agree); what each colour means is documented there.
// Pick a colour here, never a raw hex.

export const INK = {
  950: "#07080c", 900: "#0b0d13", 850: "#10131b", 800: "#151924", 700: "#1e2331", 600: "#2a3142",
  500: "#3a4357", 400: "#5d6780", 300: "#8b94ab", 200: "#b9c0d0", 100: "#e6e9f0",
} as const;
export const ACCENT = { 300: "#b5a8ff", 400: "#9b8aff", 500: "#7c6cf6", 600: "#6252e0" } as const;
export const GOOD = "#34d27b";
export const CLOSE = "#f5b83d";
export const OFF = "#f2626b";
export const ASIDE = "#f0a088";
export const NOTICE = "#fee685";
export const NOTICE_FILL = "#ffb900";

/** Every colour token by its CSS name (`--color-<name>`). */
export const TOKENS: Record<string, string> = {
  ...Object.fromEntries(Object.entries(INK).map(([k, v]) => [`ink-${k}`, v])),
  ...Object.fromEntries(Object.entries(ACCENT).map(([k, v]) => [`accent-${k}`, v])),
  good: GOOD, close: CLOSE, off: OFF, aside: ASIDE, notice: NOTICE, "notice-fill": NOTICE_FILL,
};

/** The palette the guides' drawings (components/guides) are made with, named for how they use it. */
export const GUIDE = {
  bg: INK[900],
  panel: INK[800],
  line: INK[600],
  muted: INK[300],
  text: INK[100],
  accent: ACCENT[500],
  accentLight: ACCENT[300],
  accentDark: ACCENT[600],
  good: GOOD,
  amber: CLOSE,
  red: OFF,
} as const;
