# Hayate Docs

A Fumadocs + Waku documentation and landing site for [Hayate](https://github.com/ShiinaSaku/Hayate), the encrypted LAN file transfer tool.

## Commands

Run from the `docs/` directory:

```bash
bun install
bun run dev          # localhost:8080
bun run build        # static build into dist/public
bun run types:check  # fumadocs-mdx + tsc
bun run lint         # oxlint
```

## Features

- **Fumadocs UI** default docs theme with dark/light mode and Orama search.
- **Takumi OG image generation** for `/og/image.webp` and `/og/docs/**/image.webp`.
- **LLM-friendly endpoints**: `/llms.txt`, `/llms-full.txt`, and `/docs/*.md` markdown exports.
- **SEO**: canonical links, Open Graph, Twitter cards, `robots.txt`, and `sitemap.xml`.
- **Landing page** with hero, features, install cards, and quick-start commands.

## Structure

| Path | Purpose |
|------|---------|
| `content/docs/` | MDX documentation content |
| `src/pages/` | Waku routes (landing, docs, API endpoints) |
| `src/lib/source.ts` | Fumadocs source loader + OG/markdown helpers |
| `src/components/` | Shared UI components |
| `public/` | Static assets (favicon) |
