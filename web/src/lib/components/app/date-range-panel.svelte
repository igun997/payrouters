<script lang="ts">
	// Preset list, day-range calendar and Apply/Cancel. Used inside the desktop popover and the
	// phone sheet (see date-range-filter). Draft state lives here until Apply.
	import { Button } from '#lib/components/ui/button';
	import CalendarRange from './calendar-range.svelte';
	import { rangeLabel, fullDay } from '#lib/admin-view';
	import { presetRange, todayJakarta, type RangePreset } from '#lib/format';
	import type { DateRange } from '#lib/state/shell.svelte';
	import { cn } from '#lib/utils';

	let {
		value,
		onapply,
		oncancel
	}: {
		value: DateRange;
		onapply: (range: DateRange) => void;
		oncancel: () => void;
	} = $props();

	// The panel mounts each time it opens, so the draft starts from the current range.
	// svelte-ignore state_referenced_locally
	let preset = $state<RangePreset>(value.preset);
	// svelte-ignore state_referenced_locally
	let from = $state(value.from);
	// svelte-ignore state_referenced_locally
	let to = $state(value.to);
	let picking = $state<string | null>(null);
	const max = todayJakarta();

	const presets: { value: Exclude<RangePreset, 'custom'>; label: string }[] = [
		{ value: '7d', label: 'Last 7 days' },
		{ value: '30d', label: 'Last 30 days' },
		{ value: '90d', label: 'Last 90 days' }
	];
	const short: Record<string, string> = {
		'7d': '7d',
		'30d': '30d',
		'90d': '90d',
		custom: 'Custom'
	};

	function choosePreset(next: Exclude<RangePreset, 'custom'>) {
		const range = presetRange(next);
		preset = next;
		from = range.from;
		to = range.to;
		picking = null;
	}

	function chooseCustom() {
		preset = 'custom';
		picking = null;
	}

	function pick(day: string) {
		preset = 'custom';
		if (picking === null) {
			picking = day;
			from = day;
			to = day;
			return;
		}
		const [a, b] = picking <= day ? [picking, day] : [day, picking];
		from = a;
		to = b;
		picking = null;
	}

	function apply() {
		onapply({ preset, from, to });
	}
</script>

<div class="flex flex-col gap-4 sm:flex-row" data-testid="date-range-panel">
	<div class="hidden w-40 shrink-0 flex-col gap-1 sm:flex">
		{#each presets as item (item.value)}
			<Button
				variant={preset === item.value ? 'secondary' : 'ghost'}
				class="justify-start"
				aria-pressed={preset === item.value}
				onclick={() => choosePreset(item.value)}
			>
				{item.label}
			</Button>
		{/each}
		<Button
			variant={preset === 'custom' ? 'secondary' : 'ghost'}
			class="justify-start"
			aria-pressed={preset === 'custom'}
			onclick={chooseCustom}
		>
			Custom
		</Button>
	</div>

	<div
		class="grid grid-cols-4 gap-1 rounded-full bg-secondary p-1 sm:hidden"
		role="group"
		aria-label="Preset"
	>
		{#each ['7d', '30d', '90d'] as item (item)}
			<button
				type="button"
				class={cn(
					'min-h-touch rounded-full text-tab',
					preset === item ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground'
				)}
				aria-pressed={preset === item}
				onclick={() => choosePreset(item as Exclude<RangePreset, 'custom'>)}
			>
				{short[item]}
			</button>
		{/each}
		<button
			type="button"
			class={cn(
				'min-h-touch rounded-full text-tab',
				preset === 'custom' ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground'
			)}
			aria-pressed={preset === 'custom'}
			onclick={chooseCustom}
		>
			Custom
		</button>
	</div>

	<div class="flex min-w-0 flex-1 flex-col gap-4">
		<div class="flex items-center justify-between gap-3 text-helper text-muted-foreground">
			<span>{picking ? 'Pick the end day' : 'Pick the start day'}</span>
			<span class="tabular text-mono-id font-mono text-foreground">{rangeLabel(from, to)}</span>
		</div>
		<CalendarRange {from} {to} {max} onpick={pick} />
		<p class="tabular font-mono text-helper text-muted-foreground">
			{fullDay(from)} – {fullDay(to)} · WIB
		</p>
		<div class="flex flex-col-reverse gap-2 border-t pt-4 sm:flex-row sm:justify-end">
			<Button variant="outline" onclick={oncancel}>Cancel</Button>
			<Button onclick={apply} disabled={!from || !to}>Apply range</Button>
		</div>
	</div>
</div>
