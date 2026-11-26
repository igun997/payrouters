<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import NotFoundState from '#lib/components/app/not-found-state.svelte';
	import { Button } from '#lib/components/ui/button';
	import BrandMark from '#lib/components/app/brand-mark.svelte';
	import RotateCw from '@lucide/svelte/icons/rotate-cw';

	/**
	 * Error page for everything outside the admin shell (unknown addresses, the public snap page,
	 * login). Admin pages have their own boundary in (admin)/+error.svelte, inside the shell.
	 */
	const notFound = $derived(page.status === 404);
</script>

<svelte:head>
	<title>{notFound ? 'Not found' : 'Something went wrong'} · PayRouters</title>
</svelte:head>

<main
	data-testid="page-error"
	data-status={page.status}
	class="flex min-h-svh flex-col items-center justify-center gap-6 bg-background p-page text-center"
>
	<BrandMark />
	{#if notFound}
		<div class="w-full max-w-md">
			<NotFoundState />
		</div>
	{:else}
		<div class="flex max-w-md flex-col gap-2">
			<h1 class="text-result-title">Something went wrong</h1>
			<p class="text-body text-muted-foreground">
				The page could not be loaded. Try again in a moment.
			</p>
		</div>
		<div class="flex flex-wrap justify-center gap-2">
			<Button variant="outline" onclick={() => window.location.reload()}>
				<RotateCw data-icon="inline-start" />
				Try again
			</Button>
			<Button href={resolve('/(admin)')}>Back to Overview</Button>
		</div>
	{/if}
</main>
