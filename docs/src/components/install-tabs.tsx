'use client';

import { CodeBlock, Pre } from 'fumadocs-ui/components/codeblock';
import { Tabs, Tab } from 'fumadocs-ui/components/tabs';

const commands = {
  mac: 'curl -sSf https://shiinasaku.github.io/Hayate/install.sh | bash',
  win: 'irm https://shiinasaku.github.io/Hayate/install.ps1 | iex',
  cargo: 'cargo install hayate-cli',
  termux:
    'curl -sSfL "https://github.com/ShiinaSaku/Hayate/releases/latest/download/hayate-termux-arm64" -o "$PREFIX/bin/hayate" && chmod +x "$PREFIX/bin/hayate"',
};

export function InstallTabs() {
  return (
    <Tabs
      items={['macOS / Linux', 'Windows', 'Termux', 'Cargo']}
      defaultIndex={0}
      className="w-full"
    >
      <Tab value="macOS / Linux">
        <CommandBlock command={commands.mac} />
      </Tab>
      <Tab value="Windows">
        <CommandBlock command={commands.win} />
      </Tab>
      <Tab value="Termux">
        <CommandBlock command={commands.termux} />
      </Tab>
      <Tab value="Cargo">
        <CommandBlock command={commands.cargo} />
      </Tab>
    </Tabs>
  );
}

function CommandBlock({ command }: { command: string }) {
  return (
    <div className="mt-2">
      <CodeBlock allowCopy>
        <Pre className="text-sm">
          <code>{command}</code>
        </Pre>
      </CodeBlock>
    </div>
  );
}
