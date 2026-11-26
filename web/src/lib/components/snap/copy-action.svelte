<script lang="ts">
	/*
	 * Snap-local copy button with a visible "Copied" state, a toast and a stable accessible name.
	 * Lives here (not in app/) because the customer page needs states the dashboard copy button lacks.
	 */
	import { toast } from 'svelte-sonner';
	import { Button } from '#lib/components/ui/button';
	import Check from '@lucide/svelte/icons/check';
	import Copy from '@lucide/svelte/icons/copy';

	let {
		text,
		label,
		copiedLabel = 'Copied',
		ariaLabel,
		toastMessage = 'Copied to clipboard',
		variant = 'outline',
		size = 'sm',
		testid,
		class: className
	}: {
		text: string;
		/** Visible text while idle. */
		label: string;
		copiedLabel?: string;
		/** Accessible name while idle; defaults to `label`. */
		ariaLabel?: string;
		toastMessage?: string;
		variant?: 'default' | 'outline';
		size?: 'sm' | 'lg';
		testid?: string;
		class?: string;
	} = $props();

	let copied = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;

	$effect(() => () => clearTimeout(timer));

	async function writeClipboard(value: string) {
		if (navigator.clipboard?.writeText) {
			await navigator.clipboard.writeText(value);
			return;
		}
		// Plain-HTTP or older browsers: copy through a hidden selection.
		const area = document.createElement('textarea');
		area.value = value;
		area.setAttribute('readonly', '');
		area.style.position = 'fixed';
		area.style.opacity = '0';
		document.body.append(area);
		area.select();
		const ok = document.execCommand('copy');
		area.remove();
		if (!ok) throw new Error('copy failed');
	}

	async function copy() {
		try {
			await writeClipboard(text);
			copied = true;
			toast.success(toastMessage);
			clearTimeout(timer);
			timer = setTimeout(() => (copied = false), 2000);
		} catch {
			toast.error('Copy failed. Select the text and copy it manually.');
		}
	}
</script>

<Button
	type="button"
	{variant}
	{size}
	class={className}
	data-testid={testid}
	aria-label={copied ? copiedLabel : (ariaLabel ?? label)}
	onclick={copy}
>
	{#if copied}
		<Check data-icon="inline-start" aria-hidden="true" />
	{:else}
		<Copy data-icon="inline-start" aria-hidden="true" />
	{/if}
	{copied ? copiedLabel : label}
</Button>
