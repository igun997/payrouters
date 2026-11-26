<script lang="ts">
	/* Total the customer must pay. Amount is one text node so it never wraps ("Rp" alone is a defect). */
	import type { Snippet } from 'svelte';
	import { formatIdr } from '#lib/format';
	import { cn } from '#lib/utils';

	let {
		amount,
		label = 'Total to pay',
		size = 'lg',
		actions
	}: {
		amount: number;
		label?: string;
		size?: 'xl' | 'lg';
		/** Optional control shown beside the figure (for example "Copy amount"). */
		actions?: Snippet;
	} = $props();
</script>

<section class="flex flex-wrap items-end justify-between gap-x-3 gap-y-3">
	<div class="flex min-w-0 flex-col gap-0.5">
		<span class="text-body text-muted-foreground">{label}</span>
		<span
			data-testid="snap-amount"
			class={cn(
				'tabular whitespace-nowrap text-foreground',
				size === 'xl' ? 'text-amount-xl' : 'text-amount-lg'
			)}
		>
			{formatIdr(amount)}
		</span>
	</div>
	{#if actions}{@render actions()}{/if}
</section>
