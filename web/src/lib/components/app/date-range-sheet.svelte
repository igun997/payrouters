<script lang="ts">
	// Date range sheet (design: Dashboard Sheet Date Range 390): presets, a day calendar and Apply or
	// Cancel. Apply sets the shared range; both buttons close the sheet and call `onclose`, so a
	// caller that came from another sheet can return to it.
	import ResponsiveDialog from './responsive-dialog.svelte';
	import DateRangePanel from './date-range-panel.svelte';
	import { shell, type DateRange } from '#lib/state/shell.svelte';

	let { open = $bindable(false), onclose }: { open?: boolean; onclose?: () => void } = $props();

	function commit(next: DateRange) {
		shell.setRange(next);
		open = false;
		onclose?.();
	}

	function cancel() {
		open = false;
		onclose?.();
	}
</script>

<ResponsiveDialog
	bind:open
	title="Date range"
	description="Days in Asia/Jakarta time."
	testid="sheet-date-range"
>
	<DateRangePanel value={shell.range} onapply={commit} oncancel={cancel} />
</ResponsiveDialog>
