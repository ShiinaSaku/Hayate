import { appName } from '@/lib/shared';
import { Logo } from '@/components/logo';
import { source } from '@/lib/source';
import { ImageResponse } from '@takumi-rs/image-response';
import { generate as DefaultImage } from 'fumadocs-ui/og/takumi';
import { ApiContext } from 'waku/router';

export async function GET(_: Request, { params }: ApiContext<'/og/docs/[...slugs]/image.webp'>) {
  const page = source.getPage(params.slugs);

  if (!page) return new Response(undefined, { status: 404 });

  return new ImageResponse(
    <DefaultImage
      title={page.data.title}
      description={page.data.description}
      site={appName}
      icon={<Logo className="h-16 w-16" style={{ color: '#f54e00' }} />}
      primaryColor="rgba(245,78,0,0.35)"
      primaryTextColor="#f54e00"
    />,
    {
      width: 1200,
      height: 630,
      format: 'webp',
    },
  );
}

export async function getConfig() {
  const pages = source
    .generateParams()
    .map((item) => (item.lang ? [item.lang, ...item.slug] : item.slug));

  return {
    render: 'static' as const,
    staticPaths: pages,
  } as const;
}
