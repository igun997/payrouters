<script lang="ts">
	/** Read-only value (secret, key, URL) with a Copy button. The full value is copied, never the visible part. */
	import { Input } from '#lib/components/ui/input';
	import CopyButton from '#lib/components/app/copy-button.svelte';

	let { label, value, helper }: { label: string; value: string; helper?: string } = $props();
</script>

<div class="flex flex-col gap-1.5">
	<span class="text-label text-foreground">{label}</span>
	<div class="flex items-center gap-2">
		<Input readonly {value} class="min-w-0 flex-1 font-mono" aria-label={label} />
		<CopyButton text={value} label="Copy" />
	</div>
	{#if helper}<p class="text-helper text-muted-foreground">{helper}</p>{/if}
</div>
