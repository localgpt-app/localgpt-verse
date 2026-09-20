# Reverie website

The marketing landing page and documentation for Reverie. Hand-written static
HTML/CSS/JS — no build step, no dependencies.

## View locally

```bash
cd site && python3 -m http.server 8000
# open http://localhost:8000
```

(Opening `index.html` directly from disk also works.)

## Deploy

Serve this directory from any static host (GitHub Pages, Netlify, S3, nginx).
There is nothing to compile — upload the contents of `site/` as-is.

## Structure

- `index.html` — landing page
- `docs/` — Getting started, Controls & HUD, Worlds & moods, Under the hood, Development
- `assets/css/style.css` — the design system (mirrors `src/theme.rs`)
- `assets/js/main.js` — nav toggle, hero world-cycler, scroll reveal, docs sidebar
- `assets/fonts/` — Marcellus + Hanken Grotesk (SIL OFL, see `OFL-*.txt`)

The hero cycles through the eight in-app world palettes; animations switch off
under `prefers-reduced-motion`, matching the app's Comfort settings.
