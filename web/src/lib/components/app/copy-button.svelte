<script lang="ts">
	import { toast } from 'svelte-sonner';
	import { Button } from '#lib/components/ui/button';
	import Copy from '@lucide/svelte/icons/copy';
	import Check from '@lucide/svelte/icons/check';

	let {
		text,
		label,
		variant = 'outline'
	}: {
		text: string;
		/** Button text. Omit for an icon-only button (the accessible name is "Copy"). */
		label?: string;
		variant?: 'outline' | 'ghost' | 'secondary';
	} = $props();

	let copied = $state(false);

	async function copy() {
		try {
			await navigator.clipboard.writeText(text);
			copied = true;
			toast.success('Copied to clipboard');
			setTimeout(() => (copied = false), 1500);
		} catch {
			toast.error('Copy failed. Select the text and copy it manually.');
		}
	}
</script>

<Button
	type="button"
	{variant}
	size={label ? 'sm' : 'icon-sm'}
	onclick={copy}
	aria-label={label ?? 'Copy'}
>
	{#if copied}
		<Check data-icon={label ? 'inline-start' : undefined} />
	{:else}
		<Copy data-icon={label ? 'inline-start' : undefined} />
	{/if}
	{#if label}{label}{/if}
</Button>
