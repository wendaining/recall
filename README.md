# Recall website

The English, single-page website for [Recall](https://github.com/wendaining/recall), a terminal command and output history viewer. This repository is the `gh-pages` branch of the Recall repository. The website is a static Vite, React, and TypeScript app; it does not run Recall or access a visitor's shell or command history.

## Local development

Requires Node.js 20.19+ within the 20.x line, or 22.12+ and npm.

```sh
npm install
npm run dev
```

Open the local URL printed by Vite. For a production build and local preview:

```sh
npm run typecheck
npm run build
npm run preview
```

Vite uses a relative asset base, so the built site works at a project path or at the custom domain. Building or previewing does not deploy it.

## Checks and deployment

GitHub Actions runs `npm ci` and `npm run build` on pushes and pull requests to `gh-pages`. A successful push uploads `dist/` as a GitHub Pages artifact and publishes it at [wendaining.github.io/recall/](https://wendaining.github.io/recall/); pull requests only run the checks. Run the same checks locally before releasing:

```sh
npm ci
npm run build
```

The site is published by [Cloudflare Pages Direct Upload](https://developers.cloudflare.com/pages/how-to/use-direct-upload-with-continuous-integration/). The Pages project is `recall` in the Cloudflare account that owns `wendain.ing`, its production branch is `gh-pages`, and its custom domain is [recall.wendain.ing](https://recall.wendain.ing/). To build and upload the `dist/` directory from this checkout:

```sh
npm run deploy
```

For a local Cloudflare release, authenticate once with `npx wrangler login`; Wrangler stores the OAuth login outside this repository. GitHub Pages uses the workflow's built-in `GITHUB_TOKEN`, so no Cloudflare API token needs to be added for that deployment. For a future unattended Cloudflare upload from GitHub Actions, create a Cloudflare API token with **Account → Cloudflare Pages → Edit** permission, set it as the `CLOUDFLARE_API_TOKEN` repository secret, and set `CLOUDFLARE_ACCOUNT_ID` to the account ID. Never commit tokens or `dist/`. Both hosts receive built files without replacing the source in `gh-pages`.

## Project structure

- `src/App.tsx` composes the landing page and maps each story section's scroll position to playback progress.
- `src/components/TerminalScene.tsx` renders the terminal recording from that progress, including typed commands and streamed output; `src/components/CopyCommand.tsx` handles installer command copying.
- `src/i18n/` holds the typed English copy and the locale access layer.
- `src/styles.css` defines the visual system, layout, responsive behavior, and reduced-motion rules.
- `public/` contains the local brand asset and other static files.

The terminal recording is an illustrative HTML/CSS representation of Recall's behavior, not a live terminal. Scrolling down plays it and scrolling up rewinds it; the terminal viewport advances as lines accumulate. The terminal stays visible beside the story on desktop and above the current feature explanation on narrow screens. Reduced-motion mode shows complete frames. Product claims, supported platforms, and installation commands must match the [main Recall README](https://github.com/wendaining/recall#readme).

## Adding a language

English is the only shipped language. All visible copy lives in `src/i18n/en.ts` and components access it through `src/i18n/index.ts`. To add a locale, create a dictionary with the same typed shape, register it in the locale access layer, then add a language control and update the document's `lang` attribute. Keep terminal examples localized where the text is explanatory; command syntax must remain accurate.
