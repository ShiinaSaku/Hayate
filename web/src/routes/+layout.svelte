<script lang="ts">
	import './layout.css';
	import type { LayoutProps } from './$types';
	import { onMount } from 'svelte';

	let { children }: LayoutProps = $props();

	onMount(() => {
		const stored = localStorage.getItem('hayate-theme');
		const wantsDark =
			stored === 'dark' ||
			(stored === null && window.matchMedia('(prefers-color-scheme: dark)').matches);
		document.documentElement.classList.toggle('dark', wantsDark);
	});
</script>

<svelte:head>
	<link rel="icon" href="/favicon.svg" type="image/svg+xml" />
</svelte:head>

{@render children()}
