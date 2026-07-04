import { appName, description, siteUrl } from '@/lib/shared';
import { Logo } from '@/components/logo';
import { ImageResponse } from '@takumi-rs/image-response';
import { generate as DefaultImage } from 'fumadocs-ui/og/takumi';

export async function GET() {
  return new ImageResponse(
    <DefaultImage
      title={appName}
      description={description}
      site={siteUrl.replace(/^https?:\/\//, '')}
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
  return {
    render: 'static' as const,
  } as const;
}
