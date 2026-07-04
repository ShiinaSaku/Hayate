import { appName, description, siteUrl } from '@/lib/shared';
import { ImageResponse } from '@takumi-rs/image-response';
import { generate as DefaultImage } from 'fumadocs-ui/og/takumi';

export async function GET() {
  return new ImageResponse(
    <DefaultImage
      title={appName}
      description={description}
      site={siteUrl.replace(/^https?:\/\//, '')}
    />,
    {
      width: 1200,
      height: 630,
      format: 'webp',
    },
  );
}

export async function getConfig() {
  return {
    render: 'static' as const,
  } as const;
}
