# Docs Site Guide

This is the `docs/` project for Hayate. It is a separate Waku + Fumadocs site, not part of the Cargo workspace.

## Stack

- Waku (React framework, static-first)
- Fumadocs UI / MDX / Core
- Tailwind CSS 4
- Takumi (OG image generation)
- Bun

## Commands

```bash
bun install          # install deps and generate Fumadocs MDX collections
bun run dev          # http://localhost:8080
bun run build        # static build into dist/public
bun run types:check  # fumadocs-mdx + tsc --noEmit
bun run lint         # oxlint
```

## Conventions

- Content lives in `content/docs/` as MDX files with frontmatter.
- Use `meta.json` files to define page-tree titles and grouping.
- The `icon` frontmatter should be a valid Lucide icon name (e.g. `Rocket`, `Shield`, `Terminal`).
- Waku hoists `<title>`, `<meta>`, and `<link>` tags to the document head automatically.
- Put client-side interactivity in files with a `'use client'` directive.
- OG images are generated at build time via `_api/og/` routes using Takumi and `fumadocs-ui/og/takumi`.
- LLM endpoints are statically generated at `_api/llms.txt`, `_api/llms-full.txt`, and `_api/llms.mdx/docs/[...slugs]/content.md`.

## SEO Checklist

- [ ] `title` and `description` meta on every page
- [ ] `og:image` and `twitter:image` pointing to generated Takumi images
- [ ] Canonical links (`siteUrl` in `src/lib/shared.ts`)
- [ ] `robots.txt` and `sitemap.xml`
- [ ] `/llms.txt` and `/llms-full.txt` for AI agents

## Deployment

The build outputs to `dist/public`. Deploy that folder to any static host (GitHub Pages, Cloudflare Pages, Vercel, etc.).
