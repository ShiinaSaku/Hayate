import { Link } from 'waku';
import {
  ArrowRight,
  Box,
  Cpu,
  Gauge,
  Lock,
  Package,
  Radio,
  ShieldCheck,
  Terminal,
  Zap,
} from 'lucide-react';
import { Card, Cards } from 'fumadocs-ui/components/card';
import { Callout } from 'fumadocs-ui/components/callout';
import { Steps, Step } from 'fumadocs-ui/components/steps';
import { InstallTabs } from '@/components/install-tabs';
import { CodeBlock } from '@/components/code-block';
import { Logo } from '@/components/logo';
import { appName, description, gitConfig, tagline } from '@/lib/shared';

export default function HomePage() {
  return (
    <main className="flex-1 flex flex-col">
      <title>{`${appName} — ${tagline}`}</title>
      <meta name="description" content={description} />

      <Hero />
      <FeatureGrid />
      <HowItWorks />
      <Install />
      <QuickDemo />
      <Footer />
    </main>
  );
}

function Hero() {
  return (
    <section className="relative overflow-hidden px-4 pt-20 pb-24 sm:pt-28 sm:pb-32 lg:pt-36 lg:pb-40">
      <div className="absolute inset-0 -z-10">
        <div className="absolute inset-0 bg-[radial-gradient(ellipse_80%_50%_at_50%_-20%,rgba(120,119,198,0.15),transparent)]" />
        <div className="absolute top-0 left-1/2 -translate-x-1/2 h-[600px] w-[1200px] bg-[conic-gradient(from_0deg,transparent_0_340deg,rgba(59,130,246,0.08)_360deg)] blur-3xl" />
      </div>

      <div className="mx-auto max-w-5xl">
        <div className="flex flex-col items-center gap-8 lg:flex-row lg:items-center lg:justify-between">
          <div className="max-w-2xl text-center lg:text-left">
            <div className="mb-6 inline-flex items-center gap-2 rounded-full border bg-fd-card px-3 py-1 text-sm text-fd-muted-foreground">
              <span className="relative flex h-2 w-2">
                <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-500 opacity-75" />
                <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-500" />
              </span>
              v6 is the performance release — 4 MiB frames, 8-deep read-ahead
            </div>

            <h1 className="mb-6 text-balance text-4xl font-extrabold tracking-tight text-fd-foreground sm:text-5xl lg:text-7xl">
              Send files across your LAN at
              <span className="text-fd-primary"> wire speed</span>.
            </h1>

            <p className="mx-auto mb-8 max-w-xl text-lg text-fd-muted-foreground lg:mx-0 lg:max-w-md sm:text-xl">
              {tagline} Built on QUIC, X25519, and hardware-accelerated AEAD. No cloud, no accounts, no configuration.
            </p>

            <div className="flex flex-wrap items-center justify-center gap-4 lg:justify-start">
              <Link
                to="/docs/getting-started"
                className="inline-flex items-center gap-2 rounded-lg bg-fd-primary px-6 py-3 font-semibold text-fd-primary-foreground transition hover:opacity-90"
              >
                Get Started <ArrowRight className="h-4 w-4" />
              </Link>
              <Link
                to="/docs/api/commands"
                className="inline-flex items-center gap-2 rounded-lg border px-6 py-3 font-semibold text-fd-foreground transition hover:bg-fd-muted/50"
              >
                <Terminal className="h-4 w-4" /> CLI Reference
              </Link>
            </div>

            <div className="mt-8 flex flex-wrap items-center justify-center gap-3 text-sm text-fd-muted-foreground lg:justify-start">
              <Badge href={`https://github.com/${gitConfig.user}/${gitConfig.repo}`} icon={<CodeIcon />}>
                GitHub
              </Badge>
              <Badge href="https://crates.io/crates/hayate" icon={<Package className="h-4 w-4" />}>
                crates.io
              </Badge>
              <Badge href="https://docs.rs/hayate" icon={<BookIcon />}>
                docs.rs
              </Badge>
            </div>
          </div>

          <div className="relative flex items-center justify-center lg:w-[420px]">
            <div className="absolute inset-0 rounded-full bg-fd-primary/10 blur-3xl" />
            <div className="relative rounded-3xl border bg-fd-card p-10 shadow-xl">
              <Logo className="h-40 w-40 text-fd-primary sm:h-48 sm:w-48" />
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

function Badge({ href, icon, children }: { href: string; icon: React.ReactNode; children: React.ReactNode }) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      className="inline-flex items-center gap-2 rounded-full border bg-fd-card px-4 py-1.5 transition hover:bg-fd-accent"
    >
      {icon}
      {children}
    </a>
  );
}

function CodeIcon() {
  return (
    <svg className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="16 18 22 12 16 6" />
      <polyline points="8 6 2 12 8 18" />
    </svg>
  );
}

function BookIcon() {
  return (
    <svg className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
      <path d="M6.5 2H20v20H6.5a2.5 2.5 0 0 1-2.5-2.5V4.5A2.5 2.5 0 0 1 6.5 2z" />
    </svg>
  );
}

function FeatureGrid() {
  const features = [
    {
      icon: <Gauge className="h-6 w-6 text-fd-primary" />,
      title: 'Blazing Throughput',
      description: '4 MiB frames, 8-deep read-ahead, and 64/128 MiB QUIC windows saturate 10 GbE links.',
    },
    {
      icon: <Cpu className="h-6 w-6 text-fd-primary" />,
      title: 'Kernel-Async I/O',
      description: 'Built on compio with io_uring, IOCP, or kqueue. Blocking crypto/compression runs on dedicated threads.',
    },
    {
      icon: <Radio className="h-6 w-6 text-fd-primary" />,
      title: 'Zero-Setup Discovery',
      description: 'mDNS + UDP broadcast pairing means peers find each other with a four-word code phrase.',
    },
    {
      icon: <ShieldCheck className="h-6 w-6 text-fd-primary" />,
      title: 'End-to-End Encryption',
      description: 'Ephemeral X25519 key agreement, HKDF-SHA256, and AES-256-GCM or ChaCha20-Poly1305.',
    },
    {
      icon: <Box className="h-6 w-6 text-fd-primary" />,
      title: 'Streamed Directories',
      description: 'Folders are packed and streamed as tar, with path-traversal, symlink, and hardlink rejection.',
    },
    {
      icon: <Zap className="h-6 w-6 text-fd-primary" />,
      title: 'Smart Compression',
      description: 'Zstd compression auto-skips pre-compressed formats so you do not waste CPU on archives or media.',
    },
  ];

  return (
    <section className="px-4 py-20 sm:py-28">
      <div className="mx-auto max-w-6xl">
        <div className="mb-12 text-center">
          <h2 className="text-3xl font-bold tracking-tight sm:text-4xl">Why Hayate?</h2>
          <p className="mt-4 text-fd-muted-foreground">Designed for speed, privacy, and zero friction.</p>
        </div>
        <Cards className="grid-cols-1 sm:grid-cols-2 lg:grid-cols-3">
          {features.map((feature) => (
            <Card
              key={feature.title}
              icon={feature.icon}
              title={feature.title}
              description={feature.description}
              className="bg-fd-card transition hover:border-fd-primary/30 hover:shadow-sm"
            />
          ))}
        </Cards>
      </div>
    </section>
  );
}

function HowItWorks() {
  return (
    <section className="border-y bg-fd-card/30 px-4 py-20 sm:py-28">
      <div className="mx-auto max-w-4xl">
        <div className="mb-12 text-center">
          <h2 className="text-3xl font-bold tracking-tight sm:text-4xl">Transfer in three steps</h2>
          <p className="mt-4 text-fd-muted-foreground">No configuration. No IP addresses. Just a code phrase.</p>
        </div>

        <Steps>
          <Step>
            <h3 className="text-lg font-semibold">Start the receiver</h3>
            <p className="text-fd-muted-foreground">
              The receiver opens a QUIC listener and waits for a matching sender.
            </p>
            <div className="mt-4">
              <CodeBlock code="hayate receive --code alpha-bravo-charlie-delta" title="Receiver" />
            </div>
          </Step>
          <Step>
            <h3 className="text-lg font-semibold">Send from any peer</h3>
            <p className="text-fd-muted-foreground">
              The sender broadcasts on mDNS and UDP, then negotiates an encrypted QUIC session.
            </p>
            <div className="mt-4">
              <CodeBlock code="hayate send ./photo.jpg --code alpha-bravo-charlie-delta" title="Sender" />
            </div>
          </Step>
          <Step>
            <h3 className="text-lg font-semibold">Accept and verify</h3>
            <p className="text-fd-muted-foreground">
              The receiver decrypts metadata, prompts for acceptance, and verifies the stream hash.
            </p>
            <div className="mt-4">
              <Callout type="success" title="Integrity guaranteed">
                Every transfer finishes with a BLAKE3 or SHA-256 checksum so you know the file arrived exactly as sent.
              </Callout>
            </div>
          </Step>
        </Steps>
      </div>
    </section>
  );
}

function Install() {
  return (
    <section className="px-4 py-20 sm:py-28">
      <div className="mx-auto max-w-4xl">
        <div className="mb-10 text-center">
          <h2 className="text-3xl font-bold tracking-tight sm:text-4xl">Install in seconds</h2>
          <p className="mt-4 text-fd-muted-foreground">
            One command on macOS, Linux, or Windows. Or build from source with Cargo.
          </p>
        </div>

        <InstallTabs />

        <div className="mt-8 flex items-center justify-center gap-4">
          <Link
            to="/docs/getting-started"
            className="inline-flex items-center gap-2 rounded-lg bg-fd-primary px-5 py-2.5 font-medium text-fd-primary-foreground transition hover:opacity-90"
          >
            Read the full guide <ArrowRight className="h-4 w-4" />
          </Link>
        </div>
      </div>
    </section>
  );
}

function QuickDemo() {
  return (
    <section className="px-4 py-20 sm:py-28">
      <div className="mx-auto max-w-4xl">
        <div className="overflow-hidden rounded-2xl border bg-gradient-to-br from-fd-primary/10 to-fd-primary/5 p-8 sm:p-12">
          <div className="flex flex-col items-start gap-6 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h2 className="text-2xl font-bold tracking-tight sm:text-3xl">Ready to move files?</h2>
              <p className="mt-2 max-w-md text-fd-muted-foreground">
                Check out the CLI reference or dive into the security model to see exactly how Hayate protects your data.
              </p>
            </div>
            <div className="flex flex-wrap gap-3">
              <Link
                to="/docs/security"
                className="inline-flex items-center gap-2 rounded-lg border bg-fd-background px-5 py-2.5 font-medium text-fd-foreground transition hover:bg-fd-muted"
              >
                <Lock className="h-4 w-4" /> Security
              </Link>
              <Link
                to="/docs/api/commands"
                className="inline-flex items-center gap-2 rounded-lg bg-fd-primary px-5 py-2.5 font-medium text-fd-primary-foreground transition hover:opacity-90"
              >
                <Terminal className="h-4 w-4" /> CLI
              </Link>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

function Footer() {
  return (
    <footer className="border-t px-4 py-12">
      <div className="mx-auto flex max-w-6xl flex-col items-center justify-between gap-6 sm:flex-row">
        <div className="flex items-center gap-2">
          <Logo className="h-6 w-6 text-fd-primary" />
          <span className="font-semibold">{appName}</span>
        </div>
        <p className="text-sm text-fd-muted-foreground">
          © {new Date().getFullYear()} {appName}. Open source under MIT.
        </p>
        <div className="flex gap-6 text-sm text-fd-muted-foreground">
          <a href={`https://github.com/${gitConfig.user}/${gitConfig.repo}`} className="hover:text-fd-foreground">
            GitHub
          </a>
          <a href="https://crates.io/crates/hayate" className="hover:text-fd-foreground">
            crates.io
          </a>
          <a href="https://docs.rs/hayate" className="hover:text-fd-foreground">
            docs.rs
          </a>
        </div>
      </div>
    </footer>
  );
}

export async function getConfig() {
  return {
    render: 'static' as const,
  };
}
