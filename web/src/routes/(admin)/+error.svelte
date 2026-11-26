<script lang="ts">
	import { page } from '$app/state';
	import PageHeader from '#lib/components/app/page-header.svelte';
	import NotFoundState from '#lib/components/app/not-found-state.svelte';
	import Banner from '#lib/components/app/banner.svelte';
	import { Button } from '#lib/components/ui/button';
	import RotateCw from '@lucide/svelte/icons/rotate-cw';

	/**
	 * Error boundary inside the admin shell: a 404 for an unknown dashboard address, or a failed
	 * page load. The shell (navigation, top bar) stays on screen.
	 */
	const notFound = $derived(page.status === 404);
</script>

<svelte:head>
	<title>{notFound ? 'Not found' : 'Something went wrong'} · PayRouters</title>
</svelte:head>

<div data-testid="page-error" data-status={page.status} class="flex flex-col gap-section">
	{#if notFound}
		<PageHeader title="Not found" description="This dashboard has no page at this address." />
		<NotFoundState />
	{:else}
		<PageHeader title="Something went wrong" description="This page could not be loaded." />
		<Banner tone="error" title="Request failed">
			The page could not be loaded. Try again in a moment.
			{#snippet action()}
				<Button variant="outline" size="sm" onclick={() => window.location.reload()}>
					<RotateCw data-icon="inline-start" />
					Try again
				</Button>
			{/snippet}
		</Banner>
	{/if}
</div>
