const BYTE_UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB'] as const;

export function formatBytes(value: number | null | undefined): string {
	if (!value || !Number.isFinite(value) || value <= 0) return '—';
	let size = value;
	let unit = 0;
	while (size >= 1024 && unit < BYTE_UNITS.length - 1) {
		size /= 1024;
		unit += 1;
	}
	const digits = size >= 100 || unit === 0 ? 0 : 1;
	return `${size.toFixed(digits)} ${BYTE_UNITS[unit]}`;
}

export function formatUptime(seconds: number | null | undefined): string {
	const total = Math.max(0, Math.floor(seconds ?? 0));
	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	const secs = total % 60;
	if (hours > 0) return `${hours}h ${minutes}m`;
	if (minutes > 0) return `${minutes}m ${secs}s`;
	return `${secs}s`;
}

export function formatWhen(ts: number | null | undefined): string {
	if (!ts) return '—';
	return new Date(ts * 1000).toLocaleString(undefined, {
		month: 'short',
		day: 'numeric',
		hour: '2-digit',
		minute: '2-digit'
	});
}

export function formatRtt(ms: number | null | undefined): string | null {
	if (ms == null || !Number.isFinite(ms)) return null;
	if (ms < 1) return `${Math.round(ms * 1000)} µs`;
	if (ms < 10) return `${ms.toFixed(1)} ms`;
	return `${Math.round(ms)} ms`;
}

export function cliHint(name: string): string {
	return `hayate send <path> --to ${JSON.stringify(name)}`;
}
