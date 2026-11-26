<script lang="ts">
	// Slim persistent banner shown on every admin page while the dashboard is in Live mode.
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
	import { prefs } from '#lib/state/prefs.svelte';
</script>

{#if prefs.mode === 'live'}
	<div
		role="status"
		data-testid="live-banner"
		class="flex h-9 shrink-0 items-center gap-2 bg-pending px-page text-label text-pending-foreground"
	>
		<TriangleAlert class="size-4 shrink-0" />
		<span class="truncate">Live mode: payments move real money</span>
	</div>
{/if}
