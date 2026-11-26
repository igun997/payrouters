<script lang="ts">
	// Daily bar chart in HTML and CSS, so axis labels keep their size at every width. Heights scale
	// to a nice-number axis; the peak day is the dark bar. Each bar has a title tooltip.
	import { compactRupiah } from '#lib/admin-view';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';

	export type BarDatum = { label: string; value: number; title: string };

	let {
		data,
		empty = 'No data for this range.',
		testid = 'bar-chart'
	}: {
		data: BarDatum[];
		empty?: string;
		testid?: string;
	} = $props();

	const max = $derived(Math.max(0, ...data.map((d) => d.value)));
	const hasData = $derived(data.length > 0 && max > 0);
	const peak = $derived(
		data.reduce((best, d, i) => (d.value > (data[best]?.value ?? -1) ? i : best), 0)
	);

	/** Axis ticks 0, step, 2·step … covering `top`, with a 1, 2 or 5 step. */
	function niceTicks(top: number): number[] {
		if (top <= 0) return [0];
		const raw = top / 3;
		const mag = 10 ** Math.floor(Math.log10(raw));
		const norm = raw / mag;
		const step = (norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10) * mag;
		const count = Math.ceil(top / step);
		return Array.from({ length: count + 1 }, (_, i) => i * step);
	}

	const ticks = $derived(niceTicks(max));
	const axisTop = $derived(ticks.at(-1) ?? 0);
	// Three date labels on phones, where five would run into each other.
	const phone = new IsMobile(768);
	const labelCount = $derived(Math.min(phone.current ? 3 : 5, data.length));
	const labels = $derived(
		labelCount === 0
			? []
			: Array.from(
					{ length: labelCount },
					(_, i) =>
						data[labelCount === 1 ? 0 : Math.round((i * (data.length - 1)) / (labelCount - 1))]
				)
	);

	function pct(value: number): number {
		return axisTop > 0 ? (value / axisTop) * 100 : 0;
	}
</script>

{#if hasData}
	<div class="flex flex-col gap-2 pt-5" data-testid={testid}>
		<div class="relative h-56 pl-14">
			{#each ticks as tick (tick)}
				<span
					class="tabular absolute left-0 w-12 -translate-y-1/2 text-right text-helper whitespace-nowrap text-muted-foreground"
					style="bottom: {pct(tick)}%"
				>
					{tick === 0 ? '0' : compactRupiah(tick)}
				</span>
				<div
					class={`absolute right-0 left-14 border-t ${tick === 0 ? 'border-input' : 'border-border'}`}
					style="bottom: {pct(tick)}%"
					aria-hidden="true"
				></div>
			{/each}
			<div
				class="absolute top-0 right-0 bottom-0 left-14 flex items-end gap-[3px]"
				role="img"
				aria-label="Bar chart"
			>
				{#each data as bar, i (bar.label + i)}
					<div class="flex h-full min-w-0 flex-1 items-end" title={bar.title}>
						<div
							class={`w-full rounded-t-sm ${i === peak ? 'bg-primary' : 'bg-chart-2'}`}
							style="height: {pct(bar.value)}%; min-height: {bar.value > 0 ? 2 : 0}px"
						></div>
					</div>
				{/each}
			</div>
		</div>
		<div class="tabular flex justify-between pl-14 text-helper text-muted-foreground">
			{#each labels as label, i (i)}
				<span class="whitespace-nowrap">{label.label}</span>
			{/each}
		</div>
	</div>
{:else}
	<p
		class="flex h-56 items-center justify-center text-body text-muted-foreground"
		data-testid={`${testid}-empty`}
	>
		{empty}
	</p>
{/if}
