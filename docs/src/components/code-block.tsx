import { CodeBlock as FumaCodeBlock, Pre } from 'fumadocs-ui/components/codeblock';
import { CopyButton } from './copy-button';
import { cn } from '@/lib/cn';

export function CodeBlock({
  code,
  title,
  className,
}: {
  code: string;
  title?: string;
  className?: string;
}) {
  return (
    <div
      className={cn(
        'group relative overflow-hidden rounded-xl border bg-fd-card',
        className,
      )}
    >
      {title ? (
        <div className="flex items-center justify-between border-b bg-fd-muted/40 px-4 py-2">
          <span className="text-xs font-medium uppercase tracking-wider text-fd-muted-foreground">
            {title}
          </span>
          <CopyButton text={code} />
        </div>
      ) : (
        <CopyButton text={code} />
      )}
      <div className="p-4 text-sm">
        <FumaCodeBlock allowCopy={false}>
          <Pre className="text-sm">
            <code>{code}</code>
          </Pre>
        </FumaCodeBlock>
      </div>
    </div>
  );
}

