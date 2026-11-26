<script lang="ts">
	/**
	 * Confirmation for an action that changes or removes something. Optional consequence bullets
	 * and custom content go above the buttons. A failed confirm keeps the dialog open and shows the
	 * server message in a banner (persistent, not a toast).
	 */
	import type { Snippet } from 'svelte';
	import ResponsiveDialog from '#lib/components/app/responsive-dialog.svelte';
	import Banner from '#lib/components/app/banner.svelte';
	import { Button } from '#lib/components/ui/button';
	import { errorMessage } from '#lib/api/client';

	let {
		open = $bindable(false),
		testid,
		title,
		description,
		consequences = [],
		confirmLabel,
		destructive = false,
		onconfirm,
		children
	}: {
		open?: boolean;
		testid?: string;
		title: string;
		description: string;
		consequences?: string[];
		confirmLabel: string;
		destructive?: boolean;
		/** Runs on confirm. Resolve to close the dialog; throw to keep it open with the message. */
		onconfirm: () => Promise<void>;
		children?: Snippet;
	} = $props();

	let busy = $state(false);
	let error = $state<string | null>(null);

	// A fresh open starts without the previous failure message.
	$effect(() => {
		if (open) error = null;
	});

	async function confirm() {
		busy = true;
		error = null;
		try {
			await onconfirm();
			open = false;
		} catch (cause) {
			error = errorMessage(cause);
		} finally {
			busy = false;
		}
	}
</script>

<ResponsiveDialog bind:open {title} {description} {testid}>
	{#if error}
		<Banner tone="error">{error}</Banner>
	{/if}
	{@render children?.()}
	{#if consequences.length > 0}
		<ul class="flex flex-col gap-2 text-body text-foreground">
			{#each consequences as consequence (consequence)}
				<li class="flex gap-2.5">
					<span aria-hidden="true" class="text-muted-foreground">•</span>
					<span>{consequence}</span>
				</li>
			{/each}
		</ul>
	{/if}
	{#snippet footer()}
		<Button
			variant="outline"
			class="w-full sm:w-auto"
			disabled={busy}
			onclick={() => (open = false)}
		>
			Cancel
		</Button>
		<Button
			variant={destructive ? 'destructive' : 'default'}
			class="w-full sm:w-auto"
			disabled={busy}
			onclick={confirm}
		>
			{busy ? 'Working…' : confirmLabel}
		</Button>
	{/snippet}
</ResponsiveDialog>
