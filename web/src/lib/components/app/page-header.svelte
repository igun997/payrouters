<script lang="ts">
	// Page title lives in the admin top bar (shell state). This component publishes the title,
	// description and actions to the shell while mounted, and renders an sr-only h1 for the page-title
	// hook. Actions sit in the top bar from md up (design); below md they render in the content, so
	// each action exists once at every width.
	import type { Snippet } from 'svelte';
	import { shell } from '#lib/state/shell.svelte';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';

	let {
		title,
		description,
		actions,
		rangeControl = false
	}: {
		title: string;
		description?: string;
		actions?: Snippet;
		/** Show the shared date range control in the top bar (Overview, Cash-ins). */
		rangeControl?: boolean;
	} = $props();

	const phone = new IsMobile(768);

	$effect(() => {
		shell.set({
			title,
			subtitle: description ?? null,
			rangeControl,
			actions: phone.current ? null : (actions ?? null)
		});
		return () => shell.clear();
	});
</script>

<h1 class="sr-only" data-testid="page-title">{title}</h1>
{#if actions && phone.current}
	<div class="flex flex-wrap items-center justify-end gap-2">
		{@render actions()}
	</div>
{/if}
