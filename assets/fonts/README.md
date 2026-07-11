# Fonts

Reverie's design uses two typefaces (see the design spec):

- **Marcellus** — serif display face for world names and hero titles.
- **Hanken Grotesk** — sans UI face for labels, controls, and tabular time.

Both are open-licensed (SIL Open Font License), so they can be bundled with a
distributed build. They are **not committed yet** — drop the `.ttf` files here
and Reverie picks them up automatically. Until then the app falls back to
Bevy's embedded default font (see `Fonts::load` in `src/theme.rs`).

Expected files (exact names matter):

```
Marcellus-Regular.ttf
HankenGrotesk-Regular.ttf
HankenGrotesk-Medium.ttf
HankenGrotesk-SemiBold.ttf
```

Sources (download the families, copy the weights above into this folder):

- Marcellus — https://fonts.google.com/specimen/Marcellus
- Hanken Grotesk — https://fonts.google.com/specimen/Hanken+Grotesk

Hanken Grotesk ships as a variable font on some mirrors; if you only have
`HankenGrotesk-VariableFont_wght.ttf`, either instance the static weights above
or point all three names at the variable file — Bevy will render it, just
without distinct weights.
