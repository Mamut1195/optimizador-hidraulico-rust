# docs-site — engine manual

mdBook documentation site for the `hydro-cli` engine, in English and Spanish.

## Layout

```
docs-site/
├── index.html        # bilingual landing page (language picker)
├── en/               # English book (mdBook)
│   ├── book.toml
│   ├── src/          # 17 chapter markdown files
│   └── book/         # generated HTML (gitignored)
├── es/               # Spanish book (mdBook)
│   ├── book.toml
│   ├── src/          # 17 chapter markdown files
│   └── book/         # generated HTML (gitignored)
├── build.sh          # build both books
└── serve.sh          # live-preview server (defaults to EN)
```

## Prerequisites

```bash
cargo install mdbook --locked
```

Tested with mdBook v0.5.3.

## Build

```bash
# Both languages
./docs-site/build.sh

# Single language
mdbook build docs-site/en
mdbook build docs-site/es
```

Output:

- `docs-site/en/book/index.html` — English book
- `docs-site/es/book/index.html` — Spanish book
- `docs-site/index.html` — landing page that links both books

## Live preview

```bash
# English (default)
./docs-site/serve.sh

# Spanish
./docs-site/serve.sh es
```

The book hot-reloads on every saved change to a markdown file.

## Deploy

The site is a pile of static HTML — host anywhere.

- **GitHub Pages**: push the contents of `docs-site/` to a `gh-pages` branch, or
  use a GitHub Action that runs `./docs-site/build.sh` and publishes
  `docs-site/` as the artifact.
- **S3 / Cloudflare Pages / Netlify**: upload the `docs-site/` directory
  after building both books. Make sure `index.html` is the entry point.

## Editing

Each chapter is a separate markdown file under `<lang>/src/`. Cross-chapter
links use relative paths (e.g. `./08-enums-reference.md`). Chapter order in
the sidebar is controlled by `SUMMARY.md` per language.

The numeric prefixes in filenames (`01-…`, `02-…`, `zz-appendix-a`) drive
alphabetic ordering inside the source directory and have no other meaning.

## Regenerating from the source manuals

If you ever lose the chapter files, you can re-split the canonical manuals
in `docs/`:

```bash
awk -v out=docs-site/en/src -f /tmp/split-manual.awk docs/ENGINE-MANUAL.en.md
awk -v out=docs-site/es/src -f /tmp/split-manual.awk docs/ENGINE-MANUAL.es.md
```

The splitter script is at the top of `docs-site/build.sh` (commented) for
reference.
