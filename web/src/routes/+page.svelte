<script lang="ts">
	import { onMount } from 'svelte';
	import * as Card from '$lib/components/ui/card/index.js';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Progress } from '$lib/components/ui/progress/index.js';
	import { Separator } from '$lib/components/ui/separator/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import {
		IconArrowDown,
		IconArrowUp,
		IconCheck,
		IconCopy,
		IconDeviceLaptop,
		IconFileUpload,
		IconFolderDown,
		IconHistory,
		IconLoader2,
		IconMoon,
		IconPlayerPlay,
		IconPlayerStop,
		IconRadio,
		IconSun,
		IconUpload,
		IconWifi
	} from '@tabler/icons-svelte';
	import Logo from '$lib/components/logo.svelte';
	import { formatBytes, formatRtt, formatUptime, formatWhen } from '$lib/format';

	interface Status {
		version: string;
		hostname: string;
		os: string;
		address: string;
		uptime_secs: number;
		peers: number;
		mode: string;
	}

	interface Peer {
		name: string;
		address: string;
		os: string;
		version: string;
		rtt_ms: number | null;
	}

	interface HistoryEntry {
		ts: number;
		direction: string;
		filename: string;
		size: number;
		elapsed_secs: number;
		speed_bps: number;
		checksum: string;
		cipher: string;
		peer: string;
		path: string | null;
	}

	interface Transfer {
		id: number;
		direction: 'send' | 'receive';
		filename: string;
		peer: string;
		total: number;
		transferred: number;
		status: 'uploading' | 'active' | 'done' | 'failed' | 'rejected';
		error: string | null;
		checksum: string | null;
		started: number;
		finished: number | null;
	}

	interface ReceiveState {
		running: boolean;
		stopping: boolean;
		port: number;
		dir: string;
		error: string | null;
	}

	let status = $state<Status | null>(null);
	let peers = $state<Peer[]>([]);
	let transfers = $state<Transfer[]>([]);
	let receive = $state<ReceiveState>({
		running: false,
		stopping: false,
		port: 50001,
		dir: '',
		error: null
	});
	let history = $state<HistoryEntry[]>([]);
	let connected = $state(false);
	let loading = $state(true);
	let copied = $state<string | null>(null);
	let dark = $state(false);
	let now = $state(Date.now());
	let uptimeBase = $state(0);
	let uptimeAt = $state(Date.now());

	let file = $state<File | null>(null);
	let target = $state('');
	let sending = $state(false);
	let uploadPct = $state(0);
	let sendError = $state<string | null>(null);
	let dragOver = $state(false);
	let dirDraft = $state('');
	let portDraft = $state(50001);
	let receiveBusy = $state(false);
	let completed = new Set<number>();

	const uptime = $derived(uptimeBase + Math.max(0, Math.floor((now - uptimeAt) / 1000)));
	const activeTransfers = $derived(
		transfers.filter((transfer) => transfer.status === 'uploading' || transfer.status === 'active')
	);

	onMount(() => {
		dark = document.documentElement.classList.contains('dark');

		const tick = setInterval(() => (now = Date.now()), 1000);
		const source = new EventSource('/api/events');

		source.addEventListener('state', (event) => {
			try {
				const data = JSON.parse((event as MessageEvent).data);
				status = data.status;
				peers = data.peers?.peers ?? [];
				transfers = data.transfers?.transfers ?? [];
				for (const transfer of transfers) {
					if (transfer.status === 'done' && !completed.has(transfer.id)) {
						completed.add(transfer.id);
						void loadHistory();
					}
				}
				if (data.receive) {
					receive = data.receive;
					if (!receive.running) {
						if (!dirDraft) dirDraft = receive.dir;
						portDraft = receive.port;
					}
				}
				uptimeBase = data.status?.uptime_secs ?? 0;
				uptimeAt = Date.now();
				loading = false;
				connected = true;
			} catch {
				connected = false;
			}
		});
		source.onopen = () => (connected = true);
		source.onerror = () => (connected = false);

		void loadHistory();
		const poll = setInterval(() => void loadHistory(), 15000);

		return () => {
			clearInterval(tick);
			clearInterval(poll);
			source.close();
		};
	});

	$effect(() => {
		if (!target && peers.length > 0) {
			target = peers[0].address;
		}
	});

	async function loadHistory() {
		try {
			const response = await fetch('/api/history?limit=25', { credentials: 'same-origin' });
			if (response.ok) history = await response.json();
		} catch {
			// History is best-effort; the live stream carries the rest.
		}
	}

	function pickFile(selected: File | null | undefined) {
		file = selected ?? null;
		sendError = null;
	}

	function onDrop(event: DragEvent) {
		event.preventDefault();
		dragOver = false;
		const dropped = event.dataTransfer?.files?.[0];
		if (dropped) pickFile(dropped);
	}

	function upload() {
		if (!file || !target || sending) return;
		sending = true;
		uploadPct = 0;
		sendError = null;

		const url = `/api/send?to=${encodeURIComponent(target)}&name=${encodeURIComponent(file.name)}`;
		const request = new XMLHttpRequest();
		request.open('POST', url);
		request.setRequestHeader('X-Hayate-Request', '1');
		request.upload.onprogress = (event) => {
			if (event.lengthComputable) {
				uploadPct = Math.round((event.loaded / event.total) * 100);
			}
		};
		request.onload = () => {
			sending = false;
			if (request.status >= 300) {
				sendError = errorFrom(request.responseText) ?? `send failed (${request.status})`;
				return;
			}
			uploadPct = 100;
			pickFile(null);
			const input = document.querySelector<HTMLInputElement>('#file-input');
			if (input) input.value = '';
			void loadHistory();
		};
		request.onerror = () => {
			sending = false;
			sendError = 'network error while uploading';
		};
		request.send(file);
	}

	function errorFrom(body: string): string | null {
		try {
			return JSON.parse(body)?.error ?? null;
		} catch {
			return null;
		}
	}

	async function setReceive(running: boolean) {
		receiveBusy = true;
		const base = running ? '/api/receive/start' : '/api/receive/stop';
		const query = running
			? `?port=${encodeURIComponent(String(portDraft))}&dir=${encodeURIComponent(dirDraft)}`
			: '';
		try {
			const response = await fetch(base + query, {
				method: 'POST',
				headers: { 'X-Hayate-Request': '1' },
				credentials: 'same-origin'
			});
			if (!response.ok) {
				const body = await response.text();
				sendError = null;
				receive = { ...receive, error: errorFrom(body) ?? `request failed (${response.status})` };
			}
		} catch {
			receive = { ...receive, error: 'network error' };
		} finally {
			receiveBusy = false;
		}
	}

	async function copy(value: string) {
		try {
			await navigator.clipboard.writeText(value);
			copied = value;
			setTimeout(() => {
				if (copied === value) copied = null;
			}, 1500);
		} catch {
			// Clipboard access can be denied; the address stays selectable.
		}
	}

	function toggleTheme() {
		dark = !dark;
		document.documentElement.classList.toggle('dark', dark);
		localStorage.setItem('hayate-theme', dark ? 'dark' : 'light');
	}

	function percent(transfer: Transfer): number {
		if (transfer.total === 0) return 0;
		return Math.min(100, Math.round((transfer.transferred / transfer.total) * 100));
	}

	function transferLabel(transfer: Transfer): string {
		switch (transfer.status) {
			case 'uploading':
				return 'uploading';
			case 'active':
				return transfer.direction === 'send' ? 'sending' : 'receiving';
			case 'done':
				return 'done';
			case 'failed':
				return 'failed';
			default:
				return transfer.status;
		}
	}
</script>

<svelte:head>
	<title>Hayate · dashboard</title>
	<meta name="color-scheme" content="dark light" />
</svelte:head>

<div class="min-h-screen bg-background text-foreground">
	<div class="mx-auto max-w-6xl px-5 py-8 sm:px-8">
		<header class="flex flex-wrap items-center justify-between gap-4">
			<div class="flex items-center gap-3">
				<div
					class="flex size-10 items-center justify-center rounded-xl bg-primary text-primary-foreground"
				>
					<Logo class="size-6" />
				</div>
				<div>
					<div class="flex items-center gap-2">
						<h1 class="text-lg font-semibold tracking-tight">Hayate</h1>
						{#if status}
							<Badge variant="outline" class="font-mono">v{status.version}</Badge>
						{/if}
					</div>
					<p class="text-sm text-muted-foreground">Encrypted LAN transfer</p>
				</div>
			</div>

			<div class="flex items-center gap-2">
				<Badge variant={connected ? 'secondary' : 'destructive'} class="gap-1.5">
					<span
						class="size-1.5 rounded-full {connected ? 'bg-emerald-500' : 'bg-destructive-foreground'}"
					></span>
					{connected ? 'Live' : 'Reconnecting'}
				</Badge>
				<Button variant="ghost" size="icon" onclick={toggleTheme} aria-label="Toggle theme">
					{#if dark}
						<IconSun />
					{:else}
						<IconMoon />
					{/if}
				</Button>
			</div>
		</header>

		<main class="mt-8 space-y-4">
			<section class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
				<Card.Root>
					<Card.Header class="pb-2">
						<Card.Description>Peers online</Card.Description>
						<Card.Title class="text-3xl tabular-nums">
							{#if loading}<Skeleton class="h-8 w-12" />{:else}{peers.length}{/if}
						</Card.Title>
					</Card.Header>
					<Card.Footer class="pt-0 text-xs text-muted-foreground">
						mDNS + UDP discovery
					</Card.Footer>
				</Card.Root>

				<Card.Root>
					<Card.Header class="pb-2">
						<Card.Description>Active transfers</Card.Description>
						<Card.Title class="text-3xl tabular-nums">{activeTransfers.length}</Card.Title>
					</Card.Header>
					<Card.Footer class="pt-0 text-xs text-muted-foreground">
						{receive.running ? 'Receiver is listening' : 'Receiver stopped'}
					</Card.Footer>
				</Card.Root>

				<Card.Root>
					<Card.Header class="pb-2">
						<Card.Description>Uptime</Card.Description>
						<Card.Title class="text-3xl tabular-nums">
							{#if loading}<Skeleton class="h-8 w-16" />{:else}{formatUptime(uptime)}{/if}
						</Card.Title>
					</Card.Header>
					<Card.Footer class="pt-0 text-xs text-muted-foreground">
						Dashboard process
					</Card.Footer>
				</Card.Root>

				<Card.Root>
					<Card.Header class="pb-2">
						<Card.Description>History</Card.Description>
						<Card.Title class="text-3xl tabular-nums">{history.length}</Card.Title>
					</Card.Header>
					<Card.Footer class="pt-0 text-xs text-muted-foreground">
						Last 25 recorded
					</Card.Footer>
				</Card.Root>
			</section>

			<section class="grid gap-4 lg:grid-cols-2">
				<Card.Root>
					<Card.Header>
						<Card.Title class="flex items-center gap-2">
							<IconUpload class="size-4 text-muted-foreground" />
							Send a file
						</Card.Title>
						<Card.Description>Drop a file, choose a receiver, send.</Card.Description>
					</Card.Header>
					<Card.Content class="space-y-4">
						<label
							class="flex cursor-pointer flex-col items-center gap-2 rounded-lg border border-dashed px-4 py-6 text-center transition-colors {dragOver
								? 'border-primary bg-primary/5'
								: 'border-border hover:border-muted-foreground/50'}"
							ondragover={(event) => {
								event.preventDefault();
								dragOver = true;
							}}
							ondragleave={() => (dragOver = false)}
							ondrop={onDrop}
						>
							<input
								id="file-input"
								type="file"
								class="sr-only"
								onchange={(event) => pickFile(event.currentTarget.files?.[0])}
							/>
							{#if file}
								<IconFileUpload class="size-5 text-primary" />
								<span class="text-sm font-medium">{file.name}</span>
								<span class="text-xs text-muted-foreground">{formatBytes(file.size)} · click to change</span>
							{:else}
								<IconFileUpload class="size-5 text-muted-foreground" />
								<span class="text-sm font-medium">Drop a file or browse</span>
								<span class="text-xs text-muted-foreground">Files are spooled locally, then sent over QUIC</span>
							{/if}
						</label>

						<div class="space-y-2">
							<Label for="target">Receiver</Label>
							<Input
								id="target"
								bind:value={target}
								list="peer-addresses"
								placeholder="192.168.1.50:50001"
								autocomplete="off"
							/>
							<datalist id="peer-addresses">
								{#each peers as peer (peer.address)}
									<option value={peer.address}>{peer.name}</option>
								{/each}
							</datalist>
						</div>

						{#if sending}
							<div class="space-y-1">
								<div class="flex justify-between text-xs text-muted-foreground">
									<span>Uploading…</span>
									<span class="tabular-nums">{uploadPct}%</span>
								</div>
								<Progress value={uploadPct} />
							</div>
						{/if}

						{#if sendError}
							<p class="text-sm text-destructive">{sendError}</p>
						{/if}
					</Card.Content>
					<Card.Footer class="justify-between">
						<span class="text-xs text-muted-foreground">
							{#if target}
								Sending to <span class="font-mono">{target}</span>
							{:else}
								Start a receiver or pick a peer
							{/if}
						</span>
						<Button id="send-button" onclick={upload} disabled={!file || !target || sending}>
							{#if sending}
								<IconLoader2 class="animate-spin" data-icon="inline-start" />
							{:else}
								<IconUpload data-icon="inline-start" />
							{/if}
							Send
						</Button>
					</Card.Footer>
				</Card.Root>

				<Card.Root>
					<Card.Header>
						<Card.Title class="flex items-center gap-2">
							<IconFolderDown class="size-4 text-muted-foreground" />
							Receive files
						</Card.Title>
						<Card.Description>
							Accept incoming transfers into a local folder while this page is open.
						</Card.Description>
					</Card.Header>
					<Card.Content class="space-y-4">
						<div class="flex items-center justify-between gap-3">
							<span class="text-sm text-muted-foreground">Status</span>
							<Badge variant={receive.running ? 'secondary' : 'outline'}>
								{#if receive.running}
									<IconWifi class="text-emerald-500" />
									Listening
								{:else}
									Stopped
								{/if}
							</Badge>
						</div>
						<Separator />
						<div class="grid grid-cols-[1fr_5rem] gap-3">
							<div class="space-y-2">
								<Label for="dir">Save to</Label>
								<Input id="dir" bind:value={dirDraft} disabled={receive.running} />
							</div>
							<div class="space-y-2">
								<Label for="port">Port</Label>
								<Input id="port" type="number" bind:value={portDraft} disabled={receive.running} />
							</div>
						</div>
						{#if receive.error}
							<p class="text-sm text-destructive">{receive.error}</p>
						{/if}
						{#if receive.running}
							<p class="text-xs text-muted-foreground">
								Accepting on <span class="font-mono">0.0.0.0:{receive.port}</span> — senders can
								discover this receiver.
							</p>
						{/if}
					</Card.Content>
					<Card.Footer class="justify-end">
						{#if receive.running}
							<Button
								id="receive-button"
								variant="outline"
								onclick={() => setReceive(false)}
								disabled={receiveBusy || receive.stopping}
							>
								{#if receive.stopping}
									<IconLoader2 class="animate-spin" data-icon="inline-start" />
								{:else}
									<IconPlayerStop data-icon="inline-start" />
								{/if}
								Stop
							</Button>
						{:else}
							<Button id="receive-button" onclick={() => setReceive(true)} disabled={receiveBusy}>
								{#if receiveBusy}
									<IconLoader2 class="animate-spin" data-icon="inline-start" />
								{:else}
									<IconPlayerPlay data-icon="inline-start" />
								{/if}
								Start receiving
							</Button>
						{/if}
					</Card.Footer>
				</Card.Root>
			</section>

			{#if transfers.length > 0}
				<Card.Root>
					<Card.Header>
						<Card.Title class="flex items-center gap-2">
							<IconArrowUp class="size-4 text-muted-foreground" />
							Transfers
							<Badge variant="secondary" class="ml-auto tabular-nums">{transfers.length}</Badge>
						</Card.Title>
						<Card.Description>This session, newest first.</Card.Description>
					</Card.Header>
					<Card.Content>
						<ul class="space-y-2">
							{#each transfers.slice(0, 8) as transfer (transfer.id)}
								<li class="rounded-lg border border-border bg-muted/20 px-3 py-2.5">
									<div class="flex flex-wrap items-center gap-2">
										<Badge variant={transfer.direction === 'send' ? 'secondary' : 'outline'}>
											{#if transfer.direction === 'send'}
												<IconArrowUp class="text-emerald-500" />
												Send
											{:else}
												<IconArrowDown class="text-sky-500" />
												Receive
											{/if}
										</Badge>
										<span class="min-w-0 flex-1 truncate text-sm font-medium">
											{transfer.filename}
										</span>
										<Badge
											variant={transfer.status === 'failed'
												? 'destructive'
												: transfer.status === 'done'
													? 'secondary'
													: 'outline'}
											class="text-[11px]"
										>
											{transferLabel(transfer)}
										</Badge>
									</div>
									<div class="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
										<span class="font-mono">{transfer.peer}</span>
										<span>·</span>
										<span class="tabular-nums">
											{formatBytes(transfer.transferred)} / {formatBytes(transfer.total)}
										</span>
										{#if transfer.checksum}
											<span>·</span>
											<span class="font-mono">{transfer.checksum.split('$')[0]}</span>
										{/if}
									</div>
									{#if transfer.status === 'uploading' || transfer.status === 'active'}
										<div class="mt-2">
											<Progress value={percent(transfer)} />
										</div>
									{/if}
									{#if transfer.error}
										<p class="mt-1 text-xs text-destructive">{transfer.error}</p>
									{/if}
								</li>
							{/each}
						</ul>
					</Card.Content>
				</Card.Root>
			{/if}

			<section class="grid gap-4 lg:grid-cols-[1.25fr_1fr]">
				<Card.Root>
					<Card.Header>
						<Card.Title class="flex items-center gap-2">
							<IconRadio class="size-4 text-muted-foreground" />
							Peers
							<Badge variant="secondary" class="ml-auto tabular-nums">{peers.length}</Badge>
						</Card.Title>
						<Card.Description>Receivers announcing on this network.</Card.Description>
					</Card.Header>
					<Card.Content>
						{#if loading}
							<div class="space-y-2">
								<Skeleton class="h-16 w-full" />
								<Skeleton class="h-16 w-full" />
							</div>
						{:else if peers.length === 0}
							<div
								class="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border px-4 py-10 text-center"
							>
								<IconRadio class="size-5 text-muted-foreground" />
								<p class="text-sm font-medium">No receivers right now</p>
								<p class="max-w-sm text-sm text-muted-foreground">
									Start one with <code class="rounded bg-muted px-1.5 py-0.5 font-mono text-xs"
										>hayate receive</code
									>, or press <em>Start receiving</em> above.
								</p>
							</div>
						{:else}
							<ul class="space-y-2">
								{#each peers as peer (peer.address)}
									<li
										class="flex flex-wrap items-center gap-3 rounded-lg border border-border bg-muted/30 px-3 py-2.5"
									>
										<div class="min-w-0 flex-1">
											<div class="flex items-center gap-2">
												<span class="truncate text-sm font-medium">{peer.name}</span>
												<Badge variant="outline" class="font-mono text-[11px]">
													{peer.address}
												</Badge>
											</div>
											<div class="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
												<span>{peer.os}</span>
												{#if peer.version && peer.version !== 'unknown'}
													<span>· v{peer.version}</span>
												{/if}
												{#if formatRtt(peer.rtt_ms)}
													<span>· {formatRtt(peer.rtt_ms)}</span>
												{/if}
											</div>
										</div>
										<Button variant="ghost" size="sm" onclick={() => (target = peer.address)}>
											Use
										</Button>
										<Button
											variant="ghost"
											size="icon-sm"
											onclick={() => copy(peer.address)}
											aria-label="IconCopy address"
										>
											{#if copied === peer.address}
												<IconCheck class="text-emerald-500" />
											{:else}
												<IconCopy />
											{/if}
										</Button>
									</li>
								{/each}
							</ul>
						{/if}
					</Card.Content>
				</Card.Root>

				<Card.Root>
					<Card.Header>
						<Card.Title class="flex items-center gap-2">
							<IconDeviceLaptop class="size-4 text-muted-foreground" />
							This device
						</Card.Title>
						<Card.Description>Identity advertised to peers.</Card.Description>
					</Card.Header>
					<Card.Content class="space-y-3 text-sm">
						{#if loading}
							<Skeleton class="h-24 w-full" />
						{:else if status}
							<div class="flex items-center justify-between gap-4">
								<span class="text-muted-foreground">Host</span>
								<span class="truncate font-medium">{status.hostname}</span>
							</div>
							<Separator />
							<div class="flex items-center justify-between gap-4">
								<span class="text-muted-foreground">Dashboard</span>
								<span class="font-mono text-xs">{status.address}</span>
							</div>
							<Separator />
							<div class="flex items-center justify-between gap-4">
								<span class="text-muted-foreground">Platform</span>
								<span class="font-medium">{status.os}</span>
							</div>
							<Separator />
							<div class="flex items-center justify-between gap-4">
								<span class="text-muted-foreground">Version</span>
								<span class="font-mono text-xs">v{status.version}</span>
							</div>
						{:else}
							<p class="text-muted-foreground">Waiting for the dashboard API…</p>
						{/if}
					</Card.Content>
					<Card.Footer>
						<p class="text-xs text-muted-foreground">
							Loopback only. The access token changes on every launch.
						</p>
					</Card.Footer>
				</Card.Root>
			</section>

			<Card.Root>
				<Card.Header>
					<Card.Title class="flex items-center gap-2">
						<IconHistory class="size-4 text-muted-foreground" />
						Recent transfers
					</Card.Title>
					<Card.Description>Recorded locally in the Hayate history log.</Card.Description>
				</Card.Header>
				<Card.Content>
					{#if history.length === 0}
						<div
							class="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border px-4 py-10 text-center"
						>
							<IconHistory class="size-5 text-muted-foreground" />
							<p class="text-sm font-medium">No transfers yet</p>
							<p class="text-sm text-muted-foreground">
								Completed sends and receives show up here.
							</p>
						</div>
					{:else}
						<Table.Root>
							<Table.Header>
								<Table.Row>
									<Table.Head>Direction</Table.Head>
									<Table.Head>File</Table.Head>
									<Table.Head class="text-right">Size</Table.Head>
									<Table.Head class="text-right">Speed</Table.Head>
									<Table.Head>Peer</Table.Head>
									<Table.Head class="text-right">When</Table.Head>
								</Table.Row>
							</Table.Header>
							<Table.Body>
								{#each history as entry, index (index)}
									<Table.Row>
										<Table.Cell>
											<Badge variant={entry.direction === 'send' ? 'secondary' : 'outline'}>
												{#if entry.direction === 'send'}
													<IconArrowUp class="text-emerald-500" />
													Send
												{:else}
													<IconArrowDown class="text-sky-500" />
													Receive
												{/if}
											</Badge>
										</Table.Cell>
										<Table.Cell class="max-w-[16rem] truncate font-medium">
											{entry.filename}
										</Table.Cell>
										<Table.Cell class="text-right tabular-nums">{formatBytes(entry.size)}</Table.Cell>
										<Table.Cell class="text-right tabular-nums">
											{formatBytes(entry.speed_bps)}/s
										</Table.Cell>
										<Table.Cell class="font-mono text-xs text-muted-foreground">{entry.peer}</Table.Cell>
										<Table.Cell class="text-right text-xs text-muted-foreground">
											{formatWhen(entry.ts)}
										</Table.Cell>
									</Table.Row>
								{/each}
							</Table.Body>
						</Table.Root>
					{/if}
				</Card.Content>
			</Card.Root>

			<p class="pb-4 pt-2 text-center text-xs text-muted-foreground">
				Hayate keeps everything on this machine. No cloud, no accounts.
			</p>
		</main>
	</div>
</div>
