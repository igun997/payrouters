<script lang="ts">
	// Dashboard stat. Standard card for the stat grid; `hero` is the dark phone summary card with an
	// optional detail row (Gross, Fees) under the figure.
	import TrendingUp from '@lucide/svelte/icons/trending-up';
	import TrendingDown from '@lucide/svelte/icons/trending-down';
	import { cn } from '#lib/utils';

	type Detail = { label: string; value: string };

	let {
		title,
		value,
		hint,
		change = null,
		changeLabel = 'vs previous period',
		hero = false,
		details = [],
		testid
	}: {
		title: string;
		value: string;
		hint?: string;
		/** Relative change as a fraction (0.124 = +12,4%), or null when there is nothing to compare. */
		change?: number | null;
		changeLabel?: string;
		hero?: boolean;
		details?: Detail[];
		testid?: string;
	} = $props();

	const signed = new Intl.NumberFormat('id-ID', {
		style: 'percent',
		signDisplay: 'exceptZero',
		maximumFractionDigits: 1
	});
	const changeText = $derived(change === null ? null : `${signed.format(change)} ${changeLabel}`);
	const up = $derived(change !== null && change >= 0);
</script>

{#if hero}
	<section
		class="flex flex-col rounded-xl bg-inverse p-card text-inverse-foreground"
		data-testid={testid}
	>
		<p class="text-label text-inverse-foreground/70">{title}</p>
		<div class="amount-box mt-1 text-amount-lg">
			<p class="tabular whitespace-nowrap text-signal" class:amount-fit={value.length > 12}>
				{value}
			</p>
		</div>
		{#if changeText}
			<p
				class={cn('mt-1 flex items-center gap-1.5 text-helper', up ? 'text-signal' : 'text-failed')}
			>
				{#if up}<TrendingUp class="size-3.5" />{:else}<TrendingDown class="size-3.5" />{/if}
				{changeText}
			</p>
		{/if}
		{#if details.length}
			<dl class="mt-4 grid grid-cols-2 gap-4 border-t border-inverse-foreground/15 pt-4">
				{#each details as item (item.label)}
					<div class="amount-box flex min-w-0 flex-col gap-0.5 text-card-title">
						<dt class="text-helper text-inverse-foreground/70">{item.label}</dt>
						<dd class="tabular whitespace-nowrap" class:amount-fit={item.value.length > 12}>
							{item.value}
						</dd>
					</div>
				{/each}
			</dl>
		{/if}
	</section>
{:else}
	<section
		class="flex min-w-0 flex-col gap-1 rounded-lg border border-border bg-card p-card"
		data-testid={testid}
	>
		<p class="text-label text-muted-foreground">{title}</p>
		<div class="amount-box text-stat">
			<p class="tabular whitespace-nowrap" class:amount-fit={value.length > 12}>{value}</p>
		</div>
		{#if changeText}
			<p
				class={cn(
					'flex items-center gap-1.5 text-helper',
					up ? 'text-success-foreground' : 'text-failed-foreground'
				)}
			>
				{#if up}<TrendingUp class="size-3.5" />{:else}<TrendingDown class="size-3.5" />{/if}
				{changeText}
			</p>
		{/if}
		{#if hint}
			<p class="text-helper text-muted-foreground">{hint}</p>
		{/if}
	</section>
{/if}
