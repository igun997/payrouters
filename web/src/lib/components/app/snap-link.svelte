<script lang="ts">
	// The customer-facing Snap link for a payment. Shows the path (the token) and copies the full URL.
	import CopyButton from './copy-button.svelte';
	import { fullDateTime } from '#lib/admin-view';

	let { url, expiresAt }: { url: string | null; expiresAt: string } = $props();

	const shown = $derived.by(() => {
		if (!url) return null;
		try {
			return new URL(url).pathname;
		} catch {
			return url;
		}
	});
</script>

<section
	class="flex flex-col gap-3 rounded-lg border border-border bg-card p-card"
	data-testid="snap-link"
>
	<h2 class="text-card-title">Snap page</h2>
	{#if url && shown}
		<div
			class="flex items-center gap-2 rounded-md border border-border bg-background py-1 pr-1 pl-3"
		>
			<span class="min-w-0 flex-1 truncate text-mono-id font-mono" title={url}>{shown}</span>
			<CopyButton text={url} label="Copy" />
		</div>
		<p class="text-helper text-muted-foreground">
			Send this link to the customer. It stops working for new payments after {fullDateTime(
				expiresAt
			)}.
		</p>
	{:else}
		<p class="text-body text-muted-foreground">No Snap link for this payment.</p>
	{/if}
</section>
