# TokenScavenger Marketing Website

This directory contains the marketing website source for TokenScavenger.

## Preview locally

```bash
cd docs
pnpm install
pnpm run preview
```

Open the URL shown by the development server to preview the site.

## Build for GitHub Pages

```bash
cd docs
pnpm run build
```

The production build compiles Tailwind CSS and TypeScript into `style.css` and `build/main.js`, leaving `index.html` as the site entry point for GitHub Pages.
