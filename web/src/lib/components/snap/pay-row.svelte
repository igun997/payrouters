<script lang="ts">
	/*
	 * One row of the choose-how-to-pay list: a method or a provider checkout. States: idle,
	 * connecting (spinner, the others are disabled), error (the provider refused the start; the row
	 * can be tapped again to retry). The leading tile is passed in by the caller.
	 */
	import type { Snippet } from 'svelte';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import CircleAlert from '@lucide/svelte/icons/circle-alert';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import { cn } from '#lib/utils';

	let {
		testid,
		state = 'idle',
		disabled = false,
		title,
		sub = null,
		onclick,
		leading
	}: {
		testid: string;
		state?: 'idle' | 'connecting' | 'error';
		/** Another row is starting; this one cannot be tapped. */
		disabled?: boolean;
		title: string;
		sub?: string | null;
		onclick: () => void;
		leading: Snippet;
	} = $props();
</script>

<button
	type="button"
	data-testid={testid}
	data-state={state}
	aria-busy={state === 'connecting'}
	{disabled}
	{onclick}
	class={cn(
		'flex min-h-16 w-full items-center gap-3 px-card py-3.5 text-left transition-colors hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none disabled:cursor-not-allowed sm:min-h-14 sm:py-2.5',
		disabled && state !== 'connecting' && 'opacity-60'
	)}
>
	{@render leading()}
	<span class="flex min-w-0 flex-1 flex-col gap-1">
		<span class="text-card-title font-semibold break-words text-foreground">{title}</span>
		{#if sub}
			<span
				class={cn(
					'text-helper break-words',
					state === 'error' ? 'text-failed-foreground' : 'text-muted-foreground'
				)}
			>
				{sub}
			</span>
		{/if}
	</span>
	{#if state === 'connecting'}
		<LoaderCircle
			class="size-5 shrink-0 animate-spin text-muted-foreground motion-reduce:animate-none"
			aria-hidden="true"
		/>
	{:else if state === 'error'}
		<CircleAlert class="size-5 shrink-0 text-failed-foreground" aria-hidden="true" />
	{:else}
		<ChevronRight class="size-5 shrink-0 text-muted-foreground" aria-hidden="true" />
	{/if}
</button>
