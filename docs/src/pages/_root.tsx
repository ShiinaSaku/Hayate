import type { ReactNode } from 'react';
import { Provider } from '@/components/provider';
import '@/styles/globals.css';
import {
  appName,
  description,
  homeOgImageRoute,
  siteUrl,
  twitterHandle,
} from '@/lib/shared';

export default async function RootElement({ children }: { children: ReactNode }) {
  const ogUrl = `${siteUrl}${homeOgImageRoute}`;

  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <title>{appName}</title>
        <meta name="description" content={description} />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <link rel="canonical" href={siteUrl} />
        <link rel="icon" href="/favicon.svg" type="image/svg+xml" />

        <meta property="og:site_name" content={appName} />
        <meta property="og:title" content={appName} />
        <meta property="og:description" content={description} />
        <meta property="og:image" content={ogUrl} />
        <meta property="og:image:width" content="1200" />
        <meta property="og:image:height" content="630" />
        <meta property="og:type" content="website" />
        <meta property="og:url" content={siteUrl} />

        <meta name="twitter:card" content="summary_large_image" />
        <meta name="twitter:site" content={twitterHandle} />
        <meta name="twitter:title" content={appName} />
        <meta name="twitter:description" content={description} />
        <meta name="twitter:image" content={ogUrl} />
      </head>
      <body data-version="1.0" className="flex flex-col min-h-screen">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}

export async function getConfig() {
  return {
    render: 'static' as const,
  };
}
