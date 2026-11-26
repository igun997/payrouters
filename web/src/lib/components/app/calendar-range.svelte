<script lang="ts">
	// Month grid for picking a day range, Monday first. Shows two months from sm up, one on phones.
	// Controlled: `from`/`to` are the highlighted endpoints, `onpick` receives each clicked day, and
	// days after `max` are disabled.
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import { fullDay } from '#lib/admin-view';
	import { cn } from '#lib/utils';

	let {
		from,
		to,
		max,
		onpick
	}: { from: string; to: string; max: string; onpick: (day: string) => void } = $props();

	let view = $state<string | null>(null);
	const shown = $derived(view ?? from.slice(0, 7));
	const months = $derived([shown, addMonth(shown, 1)]);
	const weekdays = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'];
	const monthFormat = new Intl.DateTimeFormat('en-GB', {
		month: 'long',
		year: 'numeric',
		timeZone: 'UTC'
	});

	function addMonth(ym: string, delta: number): string {
		const [y, m] = ym.split('-').map(Number);
		const d = new Date(Date.UTC(y, m - 1 + delta, 1));
		return `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}`;
	}

	function monthTitle(ym: string): string {
		return monthFormat.format(new Date(`${ym}-01T00:00:00Z`));
	}

	/** Day strings for a month, with nulls padding the leading and trailing weeks. */
	function cells(ym: string): (string | null)[] {
		const [y, m] = ym.split('-').map(Number);
		const days = new Date(Date.UTC(y, m, 0)).getUTCDate();
		const lead = (new Date(Date.UTC(y, m - 1, 1)).getUTCDay() + 6) % 7;
		const out: (string | null)[] = Array.from({ length: lead }, () => null);
		for (let d = 1; d <= days; d++) out.push(`${ym}-${String(d).padStart(2, '0')}`);
		while (out.length % 7 !== 0) out.push(null);
		return out;
	}

	function dayClass(day: string): string {
		if (day === from || day === to) return 'bg-primary text-primary-foreground font-semibold';
		if (day > from && day < to) return 'bg-secondary text-foreground';
		return 'text-foreground hover:bg-secondary';
	}
</script>

<div class="flex flex-col gap-3" data-testid="calendar-range">
	<div class="flex items-center justify-between gap-2">
		<button
			type="button"
			class="inline-flex size-11 items-center justify-center rounded-full bg-secondary text-foreground md:size-9"
			aria-label="Previous month"
			onclick={() => (view = addMonth(shown, -1))}
		>
			<ChevronLeft class="size-4" />
		</button>
		<button
			type="button"
			class="inline-flex size-11 items-center justify-center rounded-full bg-secondary text-foreground md:size-9"
			aria-label="Next month"
			onclick={() => (view = addMonth(shown, 1))}
		>
			<ChevronRight class="size-4" />
		</button>
	</div>
	<div class="flex gap-6">
		{#each months as ym, i (ym)}
			<div class={cn('flex min-w-0 flex-1 flex-col gap-2', i === 1 && 'hidden sm:flex')}>
				<p class="text-center text-label">{monthTitle(ym)}</p>
				<div class="grid grid-cols-7 text-center text-helper text-muted-foreground">
					{#each weekdays as wd (wd)}
						<span>{wd}</span>
					{/each}
				</div>
				<div class="grid grid-cols-7 gap-y-1">
					{#each cells(ym) as day, index (index)}
						{#if day}
							<button
								type="button"
								class={cn(
									'mx-auto flex h-11 w-full max-w-11 items-center justify-center rounded-full text-cell tabular-nums transition-colors disabled:cursor-not-allowed disabled:opacity-40',
									dayClass(day)
								)}
								disabled={day > max}
								aria-label={fullDay(day)}
								aria-pressed={day === from || day === to}
								data-day={day}
								onclick={() => onpick(day)}
							>
								{Number(day.slice(8))}
							</button>
						{:else}
							<span></span>
						{/if}
					{/each}
				</div>
			</div>
		{/each}
	</div>
</div>
