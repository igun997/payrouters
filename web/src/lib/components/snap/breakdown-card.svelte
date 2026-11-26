<script lang="ts">
	/* Label/value rows with an optional total line. Values are preformatted strings; amounts never wrap. */
	import { cn } from '#lib/utils';

	export interface BreakdownRow {
		label: string;
		value: string;
		mono?: boolean;
	}

	let {
		title = null,
		rows,
		total = null,
		class: className
	}: {
		title?: string | null;
		rows: BreakdownRow[];
		total?: BreakdownRow | null;
		class?: string;
	} = $props();
</script>

<section
	class={cn('flex flex-col gap-3 rounded-lg border border-border bg-card p-card', className)}
>
	{#if title}
		<h2 class="text-label font-semibold text-muted-foreground">{title}</h2>
	{/if}
	<dl class="flex flex-col gap-2.5 text-body">
		{#each rows as row (row.label)}
			<div class="flex items-baseline justify-between gap-4">
				<dt class="text-muted-foreground">{row.label}</dt>
				<dd class={cn('tabular text-right font-medium whitespace-nowrap', row.mono && 'font-mono')}>
					{row.value}
				</dd>
			</div>
		{/each}
		{#if total}
			<div class="flex items-baseline justify-between gap-4 border-t pt-3">
				<dt class="font-semibold">{total.label}</dt>
				<dd class="tabular text-card-title font-bold whitespace-nowrap">{total.value}</dd>
			</div>
		{/if}
	</dl>
</section>
