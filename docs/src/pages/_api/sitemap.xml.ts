import { source } from '@/lib/source';
import { siteUrl } from '@/lib/shared';

export async function GET() {
  const pages = source.getPages();
  const urls = pages.map(
    (page) => `  <url>\n    <loc>${siteUrl}${page.url}</loc>\n  </url>`,
  );

  const xml = `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n  <url>\n    <loc>${siteUrl}/</loc>\n  </url>\n${urls.join('\n')}\n</urlset>`;

  return new Response(xml, {
    headers: { 'Content-Type': 'application/xml' },
  });
}

export async function getConfig() {
  return { render: 'static' as const } as const;
}
