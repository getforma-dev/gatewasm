# FAuth Brand Reference

## Brand Colors

| Token | Hex | Usage |
|-------|-----|-------|
| Primary Gold | `#c4a265` | Main accent, interactive elements, logo |
| Light Gold | `#d4b275` | Hover states |
| Gold Dim | `rgba(196, 162, 101, 0.19)` | Backgrounds, glows |
| Gold Glow | `rgba(196, 162, 101, 0.09)` | Subtle highlights |
| Gold Border | `rgba(196, 162, 101, 0.25)` | Accent borders |

## Surface Colors (Dark Theme)

| Token | CSS Variable | Hex |
|-------|-------------|-----|
| Root/Background | `--bg` | `#0a0a0c` |
| Surface | `--surface` | `#111114` |
| Surface Raised | `--surface-raised` | `#16161a` |
| Surface Overlay | `--surface-overlay` | `#1c1c21` |
| Border | `--border` | `#1e1e24` |
| Border Accent | `--border-accent` | `#2a2a33` |

## Plane Accent Colors

| Plane | Name | Accent | Glow | Muted | Border |
|-------|------|--------|------|-------|--------|
| 1 | Operator | `#c4a265` | `rgba(196,162,101,0.25)` | `rgba(196,162,101,0.08)` | `rgba(196,162,101,0.25)` |
| 2 | Organization | `#2E9BFF` | `rgba(46,155,255,0.25)` | `rgba(46,155,255,0.08)` | `rgba(46,155,255,0.25)` |
| 3 | End User | `#1EDC82` | `rgba(30,220,130,0.25)` | `rgba(30,220,130,0.08)` | `rgba(30,220,130,0.25)` |

## Typography

| Font | Role | Weights |
|------|------|---------|
| Rajdhani | Display / UI labels | 400, 500, 600, 700 |
| DM Mono | Data / technical | 300, 400, 500 |
| Orbitron | Accent numerals | 500, 700 |
| Instrument Serif | Headings / editorial | 400, 400i |

## Logo

The FAuth mark is a shield shape with an "F" in Orbitron 700.
- Primary color: `#c4a265` (gold stroke + fill)
- Background: gradient from 14% to 3% opacity gold
- Shield path: `M6 4L94 4L100 10L100 78L50 118L0 78L0 10Z`

Note: Plane 1 (Operator) intentionally matches the brand accent gold.
The Operator plane is the "home" plane — sharing the brand color reinforces this.

### Variants
- `fauth-mark-gold.svg` — Gold on transparent (default)
- `fauth-mark-dark.svg` — Gold on dark background
- `fauth-mark-white.svg` — White for dark backgrounds

### Favicon
Shield mark at 64x64px with `#0a0a0c` fill background.
