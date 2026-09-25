时刻进行小增量的 conventional git commit，不要开始先计划好 commit 或者是等到一切都完成了才执行一次提交，务必要注意小增量。

# Recall website guidance

This branch contains the static, English landing page for [Recall](https://github.com/wendaining/recall). It is separate from the Rust application on `master`. Do not change the Rust project while working on this website.

## Source of truth

- Check the current `master` README and implementation before changing product claims, supported platforms, keyboard shortcuts, or installer commands.
- The terminal UI on the site is an illustrative demo. Never imply it runs Recall in the browser or reads a visitor's shell history.
- Reuse Recall's own logo. Borrow the scroll storytelling pattern from pi.dev, not its copy, code, or brand assets.

## Development

```sh
npm install
npm run dev
npm run typecheck
npm run build
npm run preview
```

Keep the site static and deployable at a GitHub Pages project path. Use relative asset URLs and avoid server-only behavior. Deployment is outside the current work.

## Organization and conventions

- `src/App.tsx`: page composition and active story section.
- `src/components/`: terminal scenes and reusable controls.
- `src/i18n/`: typed locale messages. Put all visible UI copy here, even while English is the only locale; do not display a language selector until another language exists.
- `src/styles.css`: visual tokens, page layout, responsive breakpoints, and reduced-motion styles.
- `public/`: local static assets.

The desktop feature story uses a sticky terminal stage synchronized with the text sections. Mobile must present each scene inline with its explanation. Use semantic sections and headings, keep links and buttons keyboard accessible, and make every feature understandable without animation. Respect `prefers-reduced-motion`. Avoid unnecessary animation libraries and keep scroll updates efficient.

Before committing, run the relevant typecheck/build and inspect desktop and mobile layouts when visual behavior changes. Use small Conventional Commits while developing, as the instruction above requires.
