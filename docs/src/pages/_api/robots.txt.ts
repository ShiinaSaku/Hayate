import { appName, description, siteUrl } from '@/lib/shared';

export function GET() {
  return new Response(
    `User-agent: *\nAllow: /\n\nSitemap: ${siteUrl}/sitemap.xml`,
    { headers: { 'Content-Type': 'text/plain' } },
  );
}

export async function getConfig() {
  return { render: 'static' as const } as const;
}
