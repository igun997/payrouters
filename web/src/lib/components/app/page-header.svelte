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
		rangeControl = false,
		phoneTitle,
		phoneDescription,
		phoneHeading
	}: {
		title: string;
		description?: string;
		actions?: Snippet;
		/** Show the shared date range control in the top bar (Overview, Cash-ins). */
		rangeControl?: boolean;
		/** Shorter top-bar title on phones, where long titles wrap. */
		phoneTitle?: string;
		/** Intro line shown in the content on phones (the top bar hides the subtitle there). */
		phoneDescription?: string;
		/** Section heading above the phone intro (the page title lives in the top bar). */
		phoneHeading?: string;
	} = $props();

	const phone = new IsMobile(768);

	$effect(() => {
		shell.set({
			title,
			phoneTitle,
			subtitle: description ?? null,
			rangeControl,
			actions: phone.current ? null : (actions ?? null)
		});
		return () => shell.clear();
	});
</script>

<h1 class="sr-only" data-testid="page-title">{title}</h1>
{#if phone.current && (phoneDescription || actions)}
	<div class="flex items-start justify-between gap-3">
		<div class="flex min-w-0 flex-1 flex-col gap-0.5">
			{#if phoneHeading}
				<h2 class="text-card-title">{phoneHeading}</h2>
			{/if}
			{#if phoneDescription}
				<p
					class={phoneHeading
						? 'text-helper text-muted-foreground'
						: 'text-body text-muted-foreground'}
					data-testid="page-intro"
				>
					{phoneDescription}
				</p>
			{/if}
		</div>
		{#if actions}
			<div class="flex shrink-0 flex-wrap items-center justify-end gap-2">
				{@render actions()}
			</div>
		{/if}
	</div>
{/if}
