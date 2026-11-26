<script lang="ts">
	import Banner from './banner.svelte';
	import { Button } from '#lib/components/ui/button';
	import { errorMessage } from '#lib/api/client';

	let { error, onretry }: { error: unknown; onretry?: () => void } = $props();
</script>

{#if error}
	<Banner tone="error" title="Request failed">
		{errorMessage(error)}
		{#snippet action()}
			{#if onretry}<Button variant="outline" size="sm" onclick={onretry}>Try again</Button>{/if}
		{/snippet}
	</Banner>
{/if}
