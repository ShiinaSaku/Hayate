import { CopyButton } from '@/components/copy-button';
import { Link } from 'waku';
import {
  ArrowRight,
  BookOpen,
  Box,
  Code,
  Cpu,
  Gauge,
  Globe,
  Lock,
  Package,
  Radio,
  ShieldCheck,
  Terminal,
  Zap,
} from 'lucide-react';
import { appName, description, gitConfig, tagline } from '@/lib/shared';

export default function HomePage() {
  return (
    <main className="flex-1 flex flex-col">
      <title>{appName} — {tagline}</title>
      <meta name="description" content={description} />

      <Hero />
      <Features />
      <Install />
      <QuickStart />
      <Footer />
    </main>
  );
}

function Hero() {
  return (
    <section className="relative overflow-hidden px-4 py-20 sm:py-28 lg:py-36">
      <div className="absolute inset-0 -z-10 bg-[radial-gradient(ellipse_80%_50%_at_50%_-20%,rgba(120,119,198,0.15),transparent)]" />

      <div className="mx-auto max-w-4xl text-center">
        <div className="mb-6 inline-flex items-center gap-2 rounded-full border bg-fd-card px-3 py-1 text-sm text-fd-muted-foreground">
          <span className="relative flex h-2 w-2">
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-500 opacity-75" />
            <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-500" />
          </span>
          v6 is the performance release — 4 MiB frames, 8-deep read-ahead
        </div>

        <h1 className="mb-6 text-balance text-4xl font-extrabold tracking-tight text-fd-foreground sm:text-6xl lg:text-7xl">
          Send files across your LAN at
          <span className="text-fd-primary"> wire speed</span>.
        </h1>

        <p className="mx-auto mb-10 max-w-2xl text-lg text-fd-muted-foreground sm:text-xl">
          {tagline} Built on QUIC, X25519, and hardware-accelerated AEAD. No cloud, no accounts, no configuration.
        </p>

        <div className="flex flex-wrap items-center justify-center gap-4">
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

        <div className="mt-10 flex flex-wrap items-center justify-center gap-3 text-sm text-fd-muted-foreground">
          <Badge href={`https://github.com/${gitConfig.user}/${gitConfig.repo}`} icon={<Code className="h-4 w-4" />}>
            GitHub
          </Badge>
          <Badge href="https://crates.io/crates/hayate" icon={<Package className="h-4 w-4" />}>
            crates.io
          </Badge>
          <Badge href="https://docs.rs/hayate" icon={<BookOpen className="h-4 w-4" />}>
            docs.rs
          </Badge>
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

function Features() {
  const features = [
    {
      icon: <Gauge className="h-6 w-6 text-fd-primary" />,
      title: 'Blazing Throughput',
      desc: '4 MiB frames, 8-deep read-ahead, and 64/128 MiB QUIC windows saturate 10 GbE links.',
    },
    {
      icon: <Cpu className="h-6 w-6 text-fd-primary" />,
      title: 'Kernel-Async I/O',
      desc: 'Built on compio with io_uring, IOCP, or kqueue. Blocking crypto/compression runs on dedicated threads.',
    },
    {
      icon: <Radio className="h-6 w-6 text-fd-primary" />,
      title: 'Zero-Setup Discovery',
      desc: 'mDNS + UDP broadcast pairing means peers find each other with a four-word code phrase.',
    },
    {
      icon: <ShieldCheck className="h-6 w-6 text-fd-primary" />,
      title: 'End-to-End Encryption',
      desc: 'Ephemeral X25519 key agreement, HKDF-SHA256, and AES-256-GCM or ChaCha20-Poly1305.',
    },
    {
      icon: <Box className="h-6 w-6 text-fd-primary" />,
      title: 'Streamed Directories',
      desc: 'Folders are packed and streamed as tar, with path-traversal, symlink, and hardlink rejection.',
    },
    {
      icon: <Zap className="h-6 w-6 text-fd-primary" />,
      title: 'Smart Compression',
      desc: 'Zstd compression auto-skips pre-compressed formats so you do not waste CPU on archives or media.',
    },
  ];

  return (
    <section className="px-4 py-16 sm:py-24">
      <div className="mx-auto max-w-6xl">
        <div className="mb-12 text-center">
          <h2 className="text-3xl font-bold tracking-tight sm:text-4xl">Why Hayate?</h2>
          <p className="mt-4 text-fd-muted-foreground">Designed for speed, privacy, and zero friction.</p>
        </div>
        <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
          {features.map((f) => (
            <div
              key={f.title}
              className="rounded-xl border bg-fd-card p-6 transition hover:border-fd-primary/30 hover:shadow-sm"
            >
              <div className="mb-4">{f.icon}</div>
              <h3 className="mb-2 font-semibold">{f.title}</h3>
              <p className="text-sm text-fd-muted-foreground">{f.desc}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function Install() {
  return (
    <section className="border-y bg-fd-card/50 px-4 py-16 sm:py-24">
      <div className="mx-auto max-w-4xl">
        <div className="mb-10 text-center">
          <h2 className="text-3xl font-bold tracking-tight sm:text-4xl">Install in seconds</h2>
          <p className="mt-4 text-fd-muted-foreground">One command on macOS, Linux, or Windows.</p>
        </div>

        <div className="grid gap-6 sm:grid-cols-2">
          <InstallCard
            title="macOS & Linux"
            icon={<Terminal className="h-5 w-5" />}
            command="curl -sSf https://shiinasaku.github.io/Hayate/install.sh | bash"
          />
          <InstallCard
            title="Windows (PowerShell)"
            icon={<Globe className="h-5 w-5" />}
            command="irm https://shiinasaku.github.io/Hayate/install.ps1 | iex"
          />
        </div>
      </div>
    </section>
  );
}

function InstallCard({ title, icon, command }: { title: string; icon: React.ReactNode; command: string }) {
  return (
    <div className="rounded-xl border bg-fd-background p-6">
      <div className="mb-4 flex items-center gap-2 font-semibold">
        {icon}
        {title}
      </div>
      <div className="relative overflow-hidden rounded-lg bg-fd-muted/50 p-4 font-mono text-sm">
        <code className="block pr-8">{command}</code>
        <CopyButton text={command} />
      </div>
    </div>
  );
}

function QuickStart() {
  return (
    <section className="px-4 py-16 sm:py-24">
      <div className="mx-auto max-w-4xl">
        <div className="mb-10 text-center">
          <h2 className="text-3xl font-bold tracking-tight sm:text-4xl">Transfer in three commands</h2>
        </div>

        <div className="grid gap-6 sm:grid-cols-3">
          <Step number={1} title="Receive" code="hayate receive --output ./downloads" />
          <Step number={2} title="Send" code="hayate send ./photo.jpg --code alpha-bravo-charlie-delta" />
          <Step number={3} title="Discover" code="hayate discover --timeout 5" />
        </div>
      </div>
    </section>
  );
}

function Step({ number, title, code }: { number: number; title: string; code: string }) {
  return (
    <div className="rounded-xl border bg-fd-card p-6">
      <div className="mb-3 flex h-8 w-8 items-center justify-center rounded-full bg-fd-primary font-bold text-fd-primary-foreground">
        {number}
      </div>
      <h3 className="mb-3 font-semibold">{title}</h3>
      <div className="rounded-lg bg-fd-muted/50 p-3 font-mono text-sm">
        <code>{code}</code>
      </div>
    </div>
  );
}

function Footer() {
  return (
    <footer className="border-t px-4 py-10">
      <div className="mx-auto flex max-w-6xl flex-col items-center justify-between gap-4 sm:flex-row">
        <div className="flex items-center gap-2">
          <Lock className="h-5 w-5 text-fd-primary" />
          <span className="font-semibold">{appName}</span>
        </div>
        <p className="text-sm text-fd-muted-foreground">
          © {new Date().getFullYear()} {appName}. Open source under MIT.
        </p>
        <div className="flex gap-4 text-sm text-fd-muted-foreground">
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
