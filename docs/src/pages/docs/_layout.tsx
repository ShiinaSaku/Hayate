import type { ReactNode } from 'react';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { GithubInfo } from 'fumadocs-ui/components/github-info';
import { source } from '@/lib/source';
import { baseOptions } from '@/lib/layout.shared';
import { gitConfig } from '@/lib/shared';

export default function Layout({ children }: { children: ReactNode }) {
  const options = baseOptions();
  return (
    <DocsLayout
      {...options}
      tree={source.getPageTree()}
      links={[
        ...(options.links ?? []),
        {
          type: 'custom',
          children: (
            <GithubInfo
              owner={gitConfig.user}
              repo={gitConfig.repo}
              className="py-1"
            />
          ),
        },
      ]}
    >
      {children}
    </DocsLayout>
  );
}
