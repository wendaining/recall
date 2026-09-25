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

Vite uses a relative asset base so the built site can later be served from a GitHub Pages project path. Building or previewing does not deploy it.

## Project structure

- `src/App.tsx` composes the landing page and its scroll-driven feature story.
- `src/components/` contains the terminal scenes and reusable UI controls.
- `src/i18n/` holds the typed English copy and the locale access layer.
- `src/styles.css` defines the visual system, layout, responsive behavior, and reduced-motion rules.
- `public/` contains the local brand asset and other static files.

The terminal scenes are illustrative HTML/CSS representations of Recall's behavior, not a live terminal. Product claims, supported platforms, and installation commands must match the [main Recall README](https://github.com/wendaining/recall#readme).

## Adding a language

English is the only shipped language. All visible copy lives in `src/i18n/en.ts` and components access it through `src/i18n/index.ts`. To add a locale, create a dictionary with the same typed shape, register it in the locale access layer, then add a language control and update the document's `lang` attribute. Keep terminal examples localized where the text is explanatory; command syntax must remain accurate.
