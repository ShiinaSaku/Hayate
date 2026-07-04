'use client';

import { useState } from 'react';
import * as TabsPrimitive from '@radix-ui/react-tabs';
import { CopyButton } from '@/components/copy-button';

const installers = [
  { value: 'macos', label: 'macOS', command: 'brew install hayate' },
  { value: 'linux', label: 'Linux', command: 'curl -fsSL https://hayate.shiina.xyz/install.sh | sh' },
  { value: 'windows', label: 'Windows', command: 'irm https://hayate.shiina.xyz/install.ps1 | iex' },
  { value: 'cargo', label: 'Cargo', command: 'cargo install hayate-cli' },
];

export function InstallTabs() {
  const [active, setActive] = useState('macos');
  const command = installers.find((i) => i.value === active)?.command ?? '';

  return (
    <div className="rounded-xl border border-hairline bg-surface-card p-6">
      <TabsPrimitive.Root
        value={active}
        onValueChange={setActive}
        className="flex flex-col gap-4"
      >
        <TabsPrimitive.List className="flex flex-wrap gap-2">
          {installers.map((installer) => (
            <TabsPrimitive.Trigger
              key={installer.value}
              value={installer.value}
              className="rounded-md px-3 py-1.5 text-sm font-medium text-muted transition data-[state=active]:bg-canvas-soft data-[state=active]:text-ink"
            >
              {installer.label}
            </TabsPrimitive.Trigger>
          ))}
        </TabsPrimitive.List>
      </TabsPrimitive.Root>
      <div className="mt-4 flex items-start justify-between gap-4 rounded-lg bg-canvas-soft p-4 code">
        <code className="text-sm text-ink break-all">{command}</code>
        <CopyButton text={command} />
      </div>
    </div>
  );
}
