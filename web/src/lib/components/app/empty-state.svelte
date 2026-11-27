<script lang="ts">
	import type { Component, Snippet } from 'svelte';
	import { cn } from '#lib/utils';

	/** Icon, heading, one line, next action. Use inside a card/region; never for loading. */
	let {
		icon: Icon,
		title,
		description,
		actions,
		dashed = false,
		class: className
	}: {
		icon?: Component<{ class?: string }>;
		title: string;
		description?: string;
		actions?: Snippet;
		dashed?: boolean;
		class?: string;
	} = $props();
</script>

<div
	data-testid="empty-state"
	class={cn(
		'flex flex-col items-center justify-center gap-3 rounded-lg px-card py-12 text-center',
		dashed && 'border border-dashed',
		className
	)}
>
	{#if Icon}
		<div class="flex size-12 items-center justify-center rounded-md bg-secondary text-foreground">
			<Icon class="size-5" />
		</div>
	{/if}
	<div class="flex max-w-sm flex-col gap-1">
		<h2 class="text-card-title">{title}</h2>
		{#if description}<p class="text-body text-muted-foreground">{description}</p>{/if}
	</div>
	{#if actions}<div
			class="mt-1 flex w-full flex-col items-center justify-center gap-2 sm:w-auto sm:flex-row sm:flex-wrap"
		>
			{@render actions()}
		</div>{/if}
</div>
