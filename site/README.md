# QRZero website

Plain HTML and CSS, no build step. `pages.yml` publishes this folder to GitHub Pages on every push to main that touches `site/`.

- The newest version number in the buttons is filled in by `app.js` from the GitHub releases API; the download links point at the latest release, so they never go stale.
- Screenshots in `img/` are WebP copies of `docs/screenshots`.
- Custom domain: set it in repo Settings > Pages (`CNAME` holds qrzero.app; the Actions deploy itself uses the Settings value).
- The three short clips in `clips/` (mp4, webm and a still for the poster) are recorded from the real app in dark mode with made-up data by `scripts/site-clips.mjs`; its header says how to run it. They download only when scrolled to, play only while on screen, and stay on their still picture (with play controls) when the visitor prefers reduced motion or has Data Saver on. Re-record them after a change to the panes they show.
