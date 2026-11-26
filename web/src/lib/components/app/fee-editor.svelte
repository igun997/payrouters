<script lang="ts">
	/**
	 * Fee schedule rows for a provider account. Each row is a method (or `*`, the default row),
	 * a fixed amount in rupiah and a percent. Rows are kept as strings while typing (see `#lib/fees`).
	 */
	import { shortMethodLabel } from '#lib/provider-display';
	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import SelectField from '#lib/components/app/select-field.svelte';
	import Plus from '@lucide/svelte/icons/plus';
	import Trash2 from '@lucide/svelte/icons/trash-2';
	import type { FeeRow } from '#lib/fees';

	type Option = { value: string; label: string };

	let {
		rows = $bindable(),
		methods
	}: {
		rows: FeeRow[];
		/** Includes the `*` default option first; labels are shortened for the rows. */
		methods: Option[];
	} = $props();

	const uid = $props.id();
	const options = $derived(
		methods.map((m) => ({ value: m.value, label: shortMethodLabel(m.label) }))
	);

	function add() {
		const used = new Set(rows.map((row) => row.method));
		const next = methods.find((m) => !used.has(m.value))?.value ?? '*';
		rows = [...rows, { method: next, fixed: '0', percent: '0' }];
	}

	function remove(index: number) {
		rows = rows.filter((_, i) => i !== index);
	}
</script>

<fieldset class="flex flex-col gap-3">
	<legend class="text-label text-foreground">Fee schedule</legend>
	<p class="-mt-2 text-helper text-muted-foreground">
		Used when the provider does not report a fee. Fixed is in rupiah; percent is e.g. 0.7. The * row
		is the default for methods without their own row.
	</p>

	{#each rows as row, i (i)}
		<div
			class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] items-end gap-2 sm:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)_minmax(0,0.8fr)_auto]"
			data-testid="fee-row"
		>
			<div class="col-span-full flex min-w-0 flex-col gap-1.5 sm:col-span-1">
				<span class="text-label text-foreground">Method</span>
				<SelectField bind:value={row.method} {options} label="Method" class="w-full" />
			</div>
			<div class="flex min-w-0 flex-col gap-1.5">
				<label class="text-label text-foreground" for="{uid}-fixed-{i}">Fixed (Rp)</label>
				<div class="relative">
					<span
						class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-helper text-muted-foreground"
					>
						Rp
					</span>
					<Input
						id="{uid}-fixed-{i}"
						type="number"
						inputmode="numeric"
						min="0"
						step="1"
						class="tabular ps-9"
						bind:value={row.fixed}
					/>
				</div>
			</div>
			<div class="flex min-w-0 flex-col gap-1.5">
				<label class="text-label text-foreground" for="{uid}-percent-{i}">Percent</label>
				<div class="relative">
					<Input
						id="{uid}-percent-{i}"
						type="number"
						inputmode="decimal"
						min="0"
						step="0.01"
						class="tabular pe-8"
						bind:value={row.percent}
					/>
					<span
						class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-helper text-muted-foreground"
					>
						%
					</span>
				</div>
			</div>
			<Button
				type="button"
				variant="ghost"
				size="icon"
				aria-label="Remove fee row"
				class="text-muted-foreground"
				onclick={() => remove(i)}
			>
				<Trash2 />
			</Button>
		</div>
	{/each}

	<div>
		<Button type="button" variant="outline" onclick={add}>
			<Plus data-icon="inline-start" />
			Add method fee
		</Button>
	</div>
</fieldset>
