<script lang="ts">
	// Phone filter sheet for Payments: status, provider, product and the date range. Changes apply as
	// they are picked; Apply closes the sheet and Reset clears the three selects (not the range).
	// The date range row closes this sheet and calls `onrange`, which opens the range sheet.
	import ResponsiveDialog from './responsive-dialog.svelte';
	import SelectField from './select-field.svelte';
	import { Button } from '#lib/components/ui/button';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';

	type Option = { value: string; label: string };

	let {
		open = $bindable(false),
		status = $bindable(''),
		provider = $bindable(''),
		productId = $bindable(''),
		statusOptions,
		providerOptions,
		productOptions,
		rangeLabel,
		onrange
	}: {
		open?: boolean;
		status?: string;
		provider?: string;
		productId?: string;
		statusOptions: Option[];
		providerOptions: Option[];
		productOptions: Option[];
		/** The shared range as text, e.g. `28 Oct – 26 Nov 2026`. */
		rangeLabel: string;
		onrange: () => void;
	} = $props();

	function reset() {
		status = '';
		provider = '';
		productId = '';
	}
</script>

<ResponsiveDialog
	bind:open
	title="Filter payments"
	description="Reset clears every filter."
	testid="sheet-payments-filter"
>
	<div class="flex flex-col gap-4">
		<div class="flex flex-col gap-1.5">
			<span class="text-label">Status</span>
			<SelectField bind:value={status} options={statusOptions} label="Status" class="w-full" />
		</div>
		<div class="flex flex-col gap-1.5">
			<span class="text-label">Provider</span>
			<SelectField
				bind:value={provider}
				options={providerOptions}
				label="Provider"
				class="w-full"
			/>
		</div>
		<div class="flex flex-col gap-1.5">
			<span class="text-label">Product</span>
			<SelectField bind:value={productId} options={productOptions} label="Product" class="w-full" />
		</div>
		<div class="flex flex-col gap-1.5">
			<span class="text-label">Date range</span>
			<button
				type="button"
				onclick={onrange}
				data-testid="filter-date-range"
				class="flex min-h-touch w-full items-center justify-between gap-3 rounded-md border border-input bg-card px-3.5 text-left text-label text-foreground hover:bg-muted"
			>
				<span class="tabular min-w-0 truncate">{rangeLabel}</span>
				<ChevronRight class="size-4 shrink-0 text-muted-foreground" />
			</button>
		</div>
	</div>
	{#snippet footer()}
		<Button class="w-full" onclick={() => (open = false)}>Apply filters</Button>
		<Button variant="outline" class="w-full" onclick={reset}>Reset</Button>
	{/snippet}
</ResponsiveDialog>
