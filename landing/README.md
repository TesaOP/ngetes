# Lumimi landing page

Static landing page for Lumimi. Plain HTML, CSS, and a small progressive-enhancement
script. No build step.

## Preview

```bash
# from this folder
python -m http.server 8080
# open http://localhost:8080
```
Or just open `index.html` in a browser.

## Deploy

Any static host works (GitHub Pages, Vercel, Netlify). For GitHub Pages, publish this
`landing/` folder (e.g. as the Pages source, or copy its contents to a `gh-pages` branch).

## Design notes

- Display type: **DuePuntoZero** (wordmark + headings). Body: system sans stack, chosen for
  fast first paint and to let the display face carry the identity.
- Palette: coral `#EC645B` (brand), deep coral `#C0392B` (coral text on cream + buttons),
  cream `#FBF6F0`, ink `#201A18`, warm dark `#1E1815`. Every text pairing was verified with
  the antislop WCAG contrast checker (AA).
- Dials: ENERGY 2 / RHYTHM 2 / MOTION 1 (hover states + one-shot scroll reveal, no loops).

## Font license (read before publishing)

`assets/fonts/` contains DuePuntoZero **Trial** weights, and the family is distributed under
**CC BY-NC** (see `Duepuntozero Family (CC BY-NC)License.pdf`). Trial + non-commercial means
this is fine for local preview, but **not** cleared to publish on a public/commercial site as-is.
Before shipping: obtain a proper license from Zetafonts, or swap the `@font-face` rules in
`styles.css` to an SIL OFL alternative. Attribution: Duepuntozero by Zetafonts.

## Placeholders and honest content

- The hero portrait, the app-preview window, and the three gallery cards are labeled
  placeholders (`TODO` comments in `index.html`) waiting for real character art / screenshots.
- Download buttons point to the GitHub releases page. There is no prebuilt binary yet, so the
  page also links to building from source.
- Navigation uses in-page anchors plus external Docs/GitHub links. Only the real GitHub link is
  shown; add YouTube/Twitter/Discord only when those accounts exist.
