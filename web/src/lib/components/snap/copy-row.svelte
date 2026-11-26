<script lang="ts">
	/* Labelled value (biller code, bill key) with a copy control. The value is set in mono for exact reading. */
	import CopyAction from './copy-action.svelte';

	let {
		label,
		value,
		testid,
		ariaLabel
	}: {
		label: string;
		value: string;
		testid?: string;
		/** Accessible name of the copy button, e.g. "Copy biller code". */
		ariaLabel: string;
	} = $props();
</script>

<div class="flex flex-col gap-2">
	<span class="text-helper text-muted-foreground">{label}</span>
	<div class="flex items-center justify-between gap-3">
		<span data-testid={testid} class="min-w-0 text-mono-va font-mono font-semibold break-all">
			{value}
		</span>
		<CopyAction text={value} label="Copy" {ariaLabel} toastMessage={`${label} copied`} />
	</div>
</div>
