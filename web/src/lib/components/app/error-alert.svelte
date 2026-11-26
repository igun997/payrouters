<script lang="ts">
	import Banner from './banner.svelte';
	import { Button } from '#lib/components/ui/button';
	import { ApiError, errorMessage } from '#lib/api/client';

	// `subject` names what failed to load ("payments"); without it the banner reports a failed action.
	let { error, subject, onretry }: { error: unknown; subject?: string; onretry?: () => void } =
		$props();

	// An unstructured HTTP failure (proxy page, bad gateway) carries no useful message of its own.
	const message = $derived(
		error instanceof ApiError && error.code === 'http_error'
			? `The server returned ${error.status}.`
			: errorMessage(error)
	);
</script>

{#if error}
	<Banner tone="error" title={subject ? `Could not load ${subject}` : 'Request failed'}>
		{message}
		{#snippet action()}
			{#if onretry}<Button variant="outline" size="sm" onclick={onretry}>Try again</Button>{/if}
		{/snippet}
	</Banner>
{/if}
