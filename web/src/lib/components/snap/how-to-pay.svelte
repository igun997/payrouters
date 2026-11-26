<script lang="ts">
	/* Per-bank payment steps as disclosures. The first guide starts open. */
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import type { Guide } from './how-to-pay-data';

	let { guides }: { guides: Guide[] } = $props();
</script>

<section
	class="flex flex-col gap-2"
	data-testid="snap-how-to-pay"
	aria-labelledby="how-to-pay-title"
>
	<h2 id="how-to-pay-title" class="text-card-title font-semibold">How to pay</h2>
	<div
		class="flex flex-col divide-y divide-border overflow-hidden rounded-lg border border-border bg-card"
	>
		{#each guides as guide, index (guide.title)}
			<details open={index === 0} class="group">
				<summary
					class="flex min-h-touch cursor-pointer list-none items-center justify-between gap-3 px-card py-3 text-body font-semibold select-none focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none [&::-webkit-details-marker]:hidden"
				>
					{guide.title}
					<ChevronDown
						class="size-4 shrink-0 transition-transform group-open:rotate-180"
						aria-hidden="true"
					/>
				</summary>
				<ol class="flex flex-col gap-3 px-card pb-4">
					{#each guide.steps as step, n (n)}
						<li class="flex gap-3 text-body">
							<span
								class="flex size-6 shrink-0 items-center justify-center rounded-full bg-secondary text-helper font-semibold"
								aria-hidden="true"
							>
								{n + 1}
							</span>
							<span>{step}</span>
						</li>
					{/each}
				</ol>
			</details>
		{/each}
	</div>
</section>
