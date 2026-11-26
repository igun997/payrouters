<script lang="ts">
	// Status filter chips with counts for the current range and filters. Scrolls sideways on phones;
	// a fade on the right edge marks more chips off-screen (the cut-off chip in the design) and
	// disappears once the row is scrolled to its end.
	import { cn } from '#lib/utils';
	import { formatInt } from '#lib/format';
	import { Skeleton } from '#lib/components/ui/skeleton';

	type Counts = Partial<Record<'' | 'paid' | 'pending' | 'expired' | 'failed', number>>;

	let {
		counts,
		value,
		onchange,
		loading = false
	}: {
		counts: Counts;
		value: string;
		onchange: (status: string) => void;
		loading?: boolean;
	} = $props();

	const chips: { value: '' | 'paid' | 'pending' | 'expired' | 'failed'; label: string }[] = [
		{ value: '', label: 'All' },
		{ value: 'paid', label: 'Paid' },
		{ value: 'pending', label: 'Pending' },
		{ value: 'expired', label: 'Expired' },
		{ value: 'failed', label: 'Failed' }
	];

	let scroller = $state<HTMLElement>();
	let more = $state(false);

	function measure() {
		if (!scroller) return;
		more = scroller.scrollWidth - scroller.clientWidth - scroller.scrollLeft > 4;
	}

	$effect(() => {
		if (!scroller) return;
		measure();
		window.addEventListener('resize', measure);
		return () => window.removeEventListener('resize', measure);
	});
</script>

<div class="relative -mx-page">
	<div
		class="overflow-x-auto px-page"
		role="group"
		aria-label="Status"
		data-scroll-x
		bind:this={scroller}
		onscroll={measure}
	>
		<div class="flex w-max gap-2 pe-6 pb-1">
			{#each chips as chip (chip.value)}
				{@const active = value === chip.value}
				<button
					type="button"
					aria-pressed={active}
					data-testid={`status-chip-${chip.value || 'all'}`}
					class={cn(
						'inline-flex h-control min-h-touch items-center gap-2 rounded-full border px-4 text-label whitespace-nowrap',
						active
							? 'border-primary bg-primary text-primary-foreground'
							: 'border-input bg-card text-foreground hover:bg-muted'
					)}
					onclick={() => onchange(chip.value)}
				>
					{chip.label}
					{#if loading && counts[chip.value] === undefined}
						<Skeleton class="h-3.5 w-5" />
					{:else if counts[chip.value] !== undefined}
						<span
							class={cn(
								'tabular text-helper',
								active ? 'text-primary-foreground/80' : 'text-muted-foreground'
							)}
						>
							{formatInt(counts[chip.value])}
						</span>
					{/if}
				</button>
			{/each}
		</div>
	</div>
	{#if more}
		<div
			class="pointer-events-none absolute inset-y-0 right-0 w-10 bg-gradient-to-l from-background to-transparent md:hidden"
			aria-hidden="true"
			data-testid="status-chips-fade"
		></div>
	{/if}
</div>
