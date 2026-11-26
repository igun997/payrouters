<script lang="ts">
	/** Destructive action set apart at the foot of an edit form. The button is always the outline-danger variant. */
	import { Button } from '#lib/components/ui/button';
	import type { Component } from 'svelte';

	let {
		title,
		description,
		actionLabel,
		icon: Icon,
		onaction
	}: {
		title: string;
		description: string;
		actionLabel: string;
		icon?: Component<{ class?: string }>;
		onaction: () => void;
	} = $props();
</script>

<section
	data-testid="danger-zone"
	class="flex flex-col gap-3 rounded-md border border-failed-foreground/40 p-card sm:flex-row sm:items-center sm:justify-between"
>
	<div class="flex min-w-0 flex-col gap-1">
		<h3 class="text-label text-failed-foreground">{title}</h3>
		<p class="text-helper text-muted-foreground">{description}</p>
	</div>
	<Button variant="destructive-outline" class="shrink-0" onclick={onaction}>
		{#if Icon}<Icon class="size-4" />{/if}
		{actionLabel}
	</Button>
</section>
