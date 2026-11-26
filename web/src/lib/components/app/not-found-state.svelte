<script lang="ts">
	import { resolve } from '$app/paths';
	import EmptyState from './empty-state.svelte';
	import { Button } from '#lib/components/ui/button';
	import FileQuestion from '@lucide/svelte/icons/file-question-mark';

	/**
	 * "Page not found" body. Used by the admin shell (`(admin)/+error.svelte`) and the root error
	 * page. `backHref` defaults to the Overview.
	 */
	let {
		title = 'Page not found',
		description = 'The link may be out of date or the page has moved. Check the address or go back to the Overview.',
		backHref,
		backLabel = 'Back to Overview'
	}: {
		title?: string;
		description?: string;
		backHref?: string;
		backLabel?: string;
	} = $props();

	const overview = resolve('/(admin)');
</script>

<div class="rounded-lg border bg-card" data-testid="not-found-state">
	<EmptyState icon={FileQuestion} {title} {description}>
		{#snippet actions()}
			<Button href={backHref ?? overview} variant="outline">{backLabel}</Button>
		{/snippet}
	</EmptyState>
</div>
