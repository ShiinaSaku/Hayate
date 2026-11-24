<script lang="ts">
	import { onMount } from 'svelte';
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
	const recentHistory = $derived(history.slice(0, 8));

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

	function clearFile() {
		pickFile(null);
		const input = document.querySelector<HTMLInputElement>('#file-input');
		if (input) input.value = '';
	}

	function usePeer(address: string) {
		target = address;
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
	<title>Hayate · local transfer console</title>
	<meta
		name="description"
		content="Send files directly across your local network. Encrypted, compressed, and verified."
	/>
	<meta name="color-scheme" content="dark light" />
</svelte:head>

<div class="shell">
	<header class="topbar">
		<div class="topbar-inner">
			<div class="brand">
				<span class="brand-mark"><Logo class="brand-logo" /></span>
				<div class="brand-text">
					<span class="brand-name">Hayate</span>
					<span class="brand-version">{status ? `v${status.version}` : 'local'}</span>
				</div>
			</div>

			<div class="top-actions">
				<span class="live" class:offline={!connected} aria-live="polite">
					<span class="live-dot"></span>
					{connected ? 'Live' : 'Reconnecting'}
				</span>
				<button class="icon-button" type="button" onclick={toggleTheme} aria-label="Toggle theme">
					{#if dark}
						<IconSun />
					{:else}
						<IconMoon />
					{/if}
				</button>
			</div>
		</div>
	</header>

	<main class="page">
		<section class="hero">
			<div class="hero-copy">
				<p class="eyebrow">Local transfer console</p>
				<h1>Files, straight <br />across the room.</h1>
				<p class="lede">
					Encrypted, compressed, and verified over your local network. No cloud, no accounts,
					no detours.
				</p>
			</div>

			<div class="metrics" aria-label="Session overview">
				<div class="metric">
					<span class="metric-label">Peers</span>
					<span class="metric-value">{loading ? '—' : peers.length}</span>
					<span class="metric-note">on this network</span>
				</div>
				<div class="metric">
					<span class="metric-label">Active</span>
					<span class="metric-value">{activeTransfers.length}</span>
					<span class="metric-note">transfers now</span>
				</div>
				<div class="metric">
					<span class="metric-label">Uptime</span>
					<span class="metric-value">{loading ? '—' : formatUptime(uptime)}</span>
					<span class="metric-note">dashboard</span>
				</div>
				<div class="metric">
					<span class="metric-label">History</span>
					<span class="metric-value">{history.length}</span>
					<span class="metric-note">recorded</span>
				</div>
			</div>
		</section>

		<section class="workspace" aria-label="Send and receive">
			<article class="panel send-panel">
				<div class="panel-head">
					<div>
						<p class="eyebrow">Send</p>
						<h2>Share a file</h2>
					</div>
					<span class="tag">QUIC · encrypted</span>
				</div>

				<label
					class="dropzone"
					class:dragging={dragOver}
					for="file-input"
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
						<span class="drop-icon selected"><IconFileUpload /></span>
						<span class="drop-title">{file.name}</span>
						<span class="drop-sub">{formatBytes(file.size)} · click to change</span>
					{:else}
						<span class="drop-icon"><IconUpload /></span>
						<span class="drop-title">Drop a file here</span>
						<span class="drop-sub">or click to browse</span>
					{/if}
				</label>

				<div class="field">
					<div class="field-label">
						<label for="target">Receiver</label>
						<span class="field-hint">Pick a peer or enter ip:port</span>
					</div>
					<div class="send-row">
						<input
							id="target"
							class="input"
							bind:value={target}
							list="peer-addresses"
							placeholder="192.168.1.50:50001"
							autocomplete="off"
							spellcheck="false"
						/>
						<datalist id="peer-addresses">
							{#each peers as peer (peer.address)}
								<option value={peer.address}>{peer.name}</option>
							{/each}
						</datalist>
						<button
							id="send-button"
							class="btn btn-primary"
							class:busy={sending}
							type="button"
							onclick={upload}
							disabled={!file || !target || sending}
						>
							{#if sending}
								<IconLoader2 class="spin" />
							{:else}
								<IconUpload />
							{/if}
							{sending ? 'Sending' : 'Send'}
						</button>
					</div>
				</div>

				{#if sending}
					<div class="progress-block">
						<div class="progress-label">
							<span>Uploading to the engine</span>
							<span class="tabular">{uploadPct}%</span>
						</div>
						<div class="progress"><span style={`width:${uploadPct}%`}></span></div>
					</div>
				{/if}

				{#if sendError}
					<p class="error" role="alert">{sendError}</p>
				{/if}

				<div class="panel-foot">
					<span>{file ? `${formatBytes(file.size)} ready to send` : 'Files are spooled locally first.'}</span>
					{#if file}
						<button class="link-button" type="button" onclick={clearFile}>Clear</button>
					{/if}
				</div>
			</article>

			<article class="panel receive-panel">
				<div class="panel-head">
					<div>
						<p class="eyebrow">Receive</p>
						<h2>Listen for files</h2>
					</div>
					<span class="status-pill" class:on={receive.running}>
						<span class="status-dot"></span>
						{receive.running ? 'Listening' : 'Stopped'}
					</span>
				</div>

				<p class="panel-copy">
					Keep this running to accept transfers into a local folder. Nothing is uploaded.
				</p>

				<div class="receive-state">
					<IconFolderDown />
					<div>
						<strong>{receive.running ? 'Ready for incoming files' : 'Not listening'}</strong>
						<span>
							{receive.running
								? `Port ${receive.port} on all interfaces`
								: 'Start a listener when you are ready to receive.'}
						</span>
					</div>
				</div>

				<div class="field-grid">
					<div class="field">
						<label for="dir">Save to</label>
						<input id="dir" class="input" bind:value={dirDraft} disabled={receive.running} />
					</div>
					<div class="field">
						<label for="port">Port</label>
						<input
							id="port"
							class="input"
							type="number"
							bind:value={portDraft}
							disabled={receive.running}
						/>
					</div>
				</div>

				{#if receive.running}
					<p class="panel-note">
						Senders can discover this device with <span class="mono">hayate discover</span>.
					</p>
				{/if}

				{#if receive.error}
					<p class="error" role="alert">{receive.error}</p>
				{/if}

				<div class="receive-actions">
					{#if receive.running}
						<button
							id="receive-button"
							class="btn btn-outline btn-block"
							type="button"
							onclick={() => setReceive(false)}
							disabled={receiveBusy || receive.stopping}
						>
							{#if receive.stopping}
								<IconLoader2 class="spin" />
							{:else}
								<IconPlayerStop />
							{/if}
							Stop receiving
						</button>
					{:else}
						<button
							id="receive-button"
							class="btn btn-primary btn-block"
							type="button"
							onclick={() => setReceive(true)}
							disabled={receiveBusy}
						>
							{#if receiveBusy}
								<IconLoader2 class="spin" />
							{:else}
								<IconPlayerPlay />
							{/if}
							Start receiving
						</button>
					{/if}
				</div>
			</article>
		</section>

		<section class="activity-grid" aria-label="Activity and devices">
			<article class="panel activity-panel">
				<div class="panel-head">
					<div>
						<p class="eyebrow">Activity</p>
						<h2>Transfers</h2>
					</div>
					<span class="tag tabular">{history.length} recorded</span>
				</div>

				{#if transfers.length > 0}
					<div class="live-list">
						{#each transfers.slice(0, 4) as transfer (transfer.id)}
							<div class="live-row">
								<span class="live-icon" class:receive={transfer.direction === 'receive'}>
									{#if transfer.direction === 'send'}
										<IconArrowUp />
									{:else}
										<IconArrowDown />
									{/if}
								</span>
								<div class="live-main">
									<div class="live-title">
										<span class="truncate">{transfer.filename}</span>
										<span class="live-status" class:failed={transfer.status === 'failed'}>
											{transferLabel(transfer)}
										</span>
									</div>
									<div class="live-meta">
										<span class="mono">{transfer.peer}</span>
										<span>·</span>
										<span class="tabular">{formatBytes(transfer.transferred)} / {formatBytes(transfer.total)}</span>
										{#if transfer.checksum}
											<span>·</span>
											<span class="mono">{transfer.checksum.split('$')[0]}</span>
										{/if}
									</div>
									{#if transfer.status === 'uploading' || transfer.status === 'active'}
										<div class="progress slim"><span style={`width:${percent(transfer)}%`}></span></div>
									{/if}
									{#if transfer.error}
										<p class="error">{transfer.error}</p>
									{/if}
								</div>
							</div>
						{/each}
					</div>
				{/if}

				{#if recentHistory.length === 0}
					<div class="empty">
						<IconHistory />
						<p>No transfers yet</p>
						<span>Completed sends and receives will appear here.</span>
					</div>
				{:else}
					<div class="subhead">
						<span>Recent</span>
						<span>Local history log</span>
					</div>
					<table class="history-table">
						<thead>
							<tr>
								<th>Direction</th>
								<th>File</th>
								<th class="num">Size</th>
								<th class="num col-speed">Speed</th>
								<th class="num">When</th>
							</tr>
						</thead>
						<tbody>
							{#each recentHistory as entry, index (index)}
								<tr>
									<td>
										<span class="direction" class:receive={entry.direction === 'receive'}>
											{#if entry.direction === 'send'}
												<IconArrowUp />
											{:else}
												<IconArrowDown />
											{/if}
											{entry.direction === 'send' ? 'Send' : 'Receive'}
										</span>
									</td>
									<td class="file-cell">
										<span>{entry.filename}</span>
									</td>
									<td class="num tabular">{formatBytes(entry.size)}</td>
									<td class="num tabular col-speed">{formatBytes(entry.speed_bps)}/s</td>
									<td class="num muted when">{formatWhen(entry.ts)}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				{/if}
			</article>

			<div class="side-stack">
				<article class="panel peers-panel">
					<div class="panel-head">
						<div>
							<p class="eyebrow">Nearby</p>
							<h2>Receivers</h2>
						</div>
						<span class="tag tabular">{peers.length}</span>
					</div>

					{#if loading}
						<div class="skeleton-list">
							<div class="skeleton"></div>
							<div class="skeleton"></div>
						</div>
					{:else if peers.length === 0}
						<div class="empty">
							<IconRadio />
							<p>No receivers yet</p>
							<span>Start one with <span class="mono">hayate receive</span>, or use the panel above.</span>
						</div>
					{:else}
						<div class="peer-list">
							{#each peers as peer (peer.address)}
								<div class="peer-row">
									<span class="peer-avatar"><IconDeviceLaptop /></span>
									<div class="peer-main">
										<div class="peer-name">{peer.name}</div>
										<div class="peer-meta">
											<span class="mono">{peer.address}</span>
											{#if peer.version && peer.version !== 'unknown'}
												<span>· v{peer.version}</span>
											{/if}
											{#if formatRtt(peer.rtt_ms)}
												<span>· {formatRtt(peer.rtt_ms)}</span>
											{/if}
										</div>
									</div>
									<div class="peer-actions">
										<button class="btn btn-ghost btn-small" type="button" onclick={() => usePeer(peer.address)}>
											Use
										</button>
										<button
											class="icon-button small"
											type="button"
											onclick={() => copy(peer.address)}
											aria-label={`Copy ${peer.address}`}
										>
											{#if copied === peer.address}
												<IconCheck class="ok" />
											{:else}
												<IconCopy />
											{/if}
										</button>
									</div>
								</div>
							{/each}
						</div>
					{/if}
				</article>

				<article class="panel device-panel">
					<div class="panel-head">
						<div>
							<p class="eyebrow">This device</p>
							<h2>Identity</h2>
						</div>
						<IconWifi class="panel-mark" />
					</div>

					{#if loading}
						<div class="skeleton-list">
							<div class="skeleton"></div>
							<div class="skeleton"></div>
							<div class="skeleton"></div>
						</div>
					{:else if status}
						<dl class="device-list">
							<div><dt>Host</dt><dd>{status.hostname}</dd></div>
							<div><dt>Dashboard</dt><dd class="mono">{status.address}</dd></div>
							<div><dt>Platform</dt><dd>{status.os}</dd></div>
							<div><dt>Version</dt><dd class="mono">v{status.version}</dd></div>
						</dl>
						<p class="panel-note">Loopback only. The access token changes on every launch.</p>
					{:else}
						<div class="empty"><p>Waiting for the dashboard API…</p></div>
					{/if}
				</article>
			</div>
		</section>

		<footer class="footer">
			<span>Hayate keeps everything on this machine.</span>
			<span>No cloud, no accounts.</span>
		</footer>
	</main>
</div>
