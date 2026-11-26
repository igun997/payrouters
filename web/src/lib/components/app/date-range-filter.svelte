<script lang="ts">
	// Shared date range (shell.range). `button` is a popover trigger with the range label, used in
	// the desktop top bar and filter rows. `segmented` is the phone control: 7d / 30d / 90d / Custom,
	// with Custom opening the range sheet.
	import { Popover } from 'bits-ui';
	import * as ToggleGroup from '#lib/components/ui/toggle-group';
	import DateRangeSheet from './date-range-sheet.svelte';
	import DateRangePanel from './date-range-panel.svelte';
	import { rangeLabel } from '#lib/admin-view';
	import { presetRange, type RangePreset } from '#lib/format';
	import { shell, type DateRange } from '#lib/state/shell.svelte';
	import CalendarDays from '@lucide/svelte/icons/calendar-days';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';

	let { variant = 'button' }: { variant?: 'button' | 'segmented' } = $props();

	let popoverOpen = $state(false);
	let sheetOpen = $state(false);

	const segmentClass =
		'min-h-touch rounded-md border-transparent bg-transparent px-2 text-label font-semibold text-muted-foreground shadow-none hover:text-foreground data-[state=on]:bg-card data-[state=on]:text-foreground data-[state=on]:shadow-sm';

	function commit(next: DateRange) {
		shell.setRange(next);
		popoverOpen = false;
	}

	function choose(value: string) {
		if (value === 'custom') {
			sheetOpen = true;
			return;
		}
		if (value === '7d' || value === '30d' || value === '90d') {
			const range = presetRange(value satisfies Exclude<RangePreset, 'custom'>);
			shell.setRange({ preset: value, from: range.from, to: range.to });
		}
	}
</script>

{#if variant === 'button'}
	<Popover.Root bind:open={popoverOpen}>
		<Popover.Trigger
			class="inline-flex h-control items-center gap-2 rounded-md border border-input bg-card px-3.5 text-label text-foreground hover:bg-muted"
			data-testid="date-range-trigger"
		>
			<CalendarDays class="size-4 text-muted-foreground" />
			<span class="tabular whitespace-nowrap">{rangeLabel(shell.range.from, shell.range.to)}</span>
			<ChevronDown class="size-4 text-muted-foreground" />
		</Popover.Trigger>
		<Popover.Portal>
			<Popover.Content
				align="end"
				sideOffset={8}
				class="z-50 max-h-[min(var(--bits-popover-content-available-height),calc(100svh-2rem))] w-[min(640px,calc(100vw-2rem))] overflow-y-auto rounded-xl border bg-popover p-card text-popover-foreground shadow-lg outline-none"
				data-testid="date-range-popover"
			>
				<DateRangePanel
					value={shell.range}
					onapply={commit}
					oncancel={() => (popoverOpen = false)}
				/>
			</Popover.Content>
		</Popover.Portal>
	</Popover.Root>
{:else}
	<ToggleGroup.Root
		type="single"
		variant="default"
		size="sm"
		spacing={4}
		value={shell.range.preset}
		onValueChange={choose}
		aria-label="Date range"
		class="grid w-full grid-cols-4 gap-1 rounded-lg bg-secondary p-1"
		data-testid="date-range-segmented"
	>
		<ToggleGroup.Item value="7d" class={segmentClass}>7d</ToggleGroup.Item>
		<ToggleGroup.Item value="30d" class={segmentClass}>30d</ToggleGroup.Item>
		<ToggleGroup.Item value="90d" class={segmentClass}>90d</ToggleGroup.Item>
		<ToggleGroup.Item value="custom" class={segmentClass}>Custom</ToggleGroup.Item>
	</ToggleGroup.Root>

	<DateRangeSheet bind:open={sheetOpen} />
{/if}
