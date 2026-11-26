<script lang="ts">
	// Breakdown rows (provider share of gross). Rows at md and xl, three tiles from lg to xl.
	import ProviderMark from './provider-mark.svelte';

	export type BarListRow = {
		key: string;
		label: string;
		provider: string;
		amount: number;
		/** Share of the total as a ratio 0..1. */
		share: number;
		detail: string;
	};

	let {
		rows,
		format,
		empty = 'No data for this range.',
		testid = 'bar-list'
	}: {
		rows: BarListRow[];
		format: (value: number) => string;
		empty?: string;
		testid?: string;
	} = $props();
</script>

{#if rows.length === 0}
	<p class="py-6 text-center text-body text-muted-foreground" data-testid={`${testid}-empty`}>
		{empty}
	</p>
{:else}
	<ul class="grid gap-section lg:grid-cols-3 xl:grid-cols-1" data-testid={testid}>
		{#each rows as row (row.key)}
			<li
				class="flex min-w-0 flex-col gap-2.5 lg:rounded-lg lg:border lg:border-border lg:p-card xl:border-0 xl:p-0"
				data-testid="bar-row"
			>
				<div class="flex min-w-0 items-center gap-3">
					<ProviderMark provider={row.provider} />
					<div class="flex min-w-0 flex-1 flex-col">
						<span class="text-label break-words">{row.label}</span>
						<span class="text-helper break-words text-muted-foreground">{row.detail}</span>
					</div>
					<span class="tabular shrink-0 text-card-title whitespace-nowrap">
						{format(row.amount)}
					</span>
				</div>
				<div class="h-2 w-full overflow-hidden rounded-full bg-secondary" aria-hidden="true">
					<div
						class="h-full rounded-full bg-primary"
						style="width: {Math.round(row.share * 100)}%"
					></div>
				</div>
			</li>
		{/each}
	</ul>
{/if}
