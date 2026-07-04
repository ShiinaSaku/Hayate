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
      icon={<Logo className="h-14 w-14 text-cyan-400" />}
      primaryColor="rgba(6,182,212,0.35)"
      primaryTextColor="#22d3ee"
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
