<script lang="ts">
	// Dark totals strip (cash-ins recap). Phones: the emphasised item (net received) spans both
	// columns and comes first, with a divider under it. From md: four columns in the given order,
	// the emphasised item in the signal colour.
	import { cn } from '#lib/utils';
	import { Skeleton } from '#lib/components/ui/skeleton';

	type Item = { label: string; value: string; emphasis?: boolean };

	let {
		items,
		loading = false,
		testid = 'totals-strip'
	}: { items: Item[]; loading?: boolean; testid?: string } = $props();
</script>

<dl
	class="grid grid-cols-2 gap-x-6 gap-y-4 rounded-xl bg-inverse p-card text-inverse-foreground md:grid-cols-4 md:divide-x md:divide-inverse-foreground/15"
	data-testid={testid}
>
	{#each items as item (item.label)}
		<div
			class={cn(
				'amount-box flex min-w-0 flex-col gap-1 text-card-title md:px-6 md:first:pl-0',
				item.emphasis &&
					'order-first col-span-2 border-b border-inverse-foreground/15 pb-4 md:order-none md:col-span-1 md:border-b-0 md:pb-0'
			)}
		>
			<dt class="text-helper text-inverse-foreground/70">{item.label}</dt>
			<dd
				class={cn('tabular whitespace-nowrap', item.emphasis && 'text-signal')}
				class:amount-fit={item.value.length > 12}
			>
				{#if loading}
					<Skeleton class="h-5 w-28 bg-inverse-foreground/15" />
				{:else}
					{item.value}
				{/if}
			</dd>
		</div>
	{/each}
</dl>
