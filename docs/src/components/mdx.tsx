import defaultMdxComponents from 'fumadocs-ui/mdx';
import { Tabs, Tab } from 'fumadocs-ui/components/tabs';
import { Steps, Step } from 'fumadocs-ui/components/steps';
import { Files, File, Folder } from 'fumadocs-ui/components/files';
import { TypeTable } from 'fumadocs-ui/components/type-table';
import { InlineTOC } from 'fumadocs-ui/components/inline-toc';
import { DynamicCodeBlock } from 'fumadocs-ui/components/dynamic-codeblock';
import { ServerCodeBlock } from 'fumadocs-ui/components/codeblock.rsc';
import type { MDXComponents } from 'mdx/types';

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    Tabs,
    Tab,
    Steps,
    Step,
    Files,
    File,
    Folder,
    TypeTable,
    InlineTOC,
    DynamicCodeBlock,
    ServerCodeBlock,
    ...components,
  } satisfies MDXComponents;
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
