<script lang="ts">
	/**
	 * Shows a secret that is displayed exactly once (webhook secret, API key). The dialog has no
	 * way back to the value: closing it loses it, so the banner says so. `signature` adds the
	 * verification snippet used for webhook secrets.
	 */
	import ResponsiveDialog from '#lib/components/app/responsive-dialog.svelte';
	import Banner from '#lib/components/app/banner.svelte';
	import CopyField from '#lib/components/app/copy-field.svelte';
	import { Button } from '#lib/components/ui/button';
	import CircleCheck from '@lucide/svelte/icons/circle-check';

	let {
		open = $bindable(false),
		testid,
		title,
		description,
		successTitle,
		bannerTitle = 'This secret is shown once',
		bannerBody,
		label,
		value,
		helper,
		signature = false,
		doneLabel = 'Done'
	}: {
		open?: boolean;
		testid?: string;
		title: string;
		description: string;
		/** Optional success row above the banner, e.g. "Webhook secret for Demo Shop". */
		successTitle?: string;
		bannerTitle?: string;
		bannerBody: string;
		label: string;
		value: string;
		helper?: string;
		signature?: boolean;
		doneLabel?: string;
	} = $props();
</script>

<ResponsiveDialog bind:open {title} {description} {testid}>
	{#if successTitle}
		<div class="flex items-center gap-3">
			<CircleCheck class="size-6 shrink-0 text-success-foreground" />
			<span class="text-label text-foreground">{successTitle}</span>
		</div>
	{/if}
	<Banner tone="info" title={bannerTitle}>{bannerBody}</Banner>
	<CopyField {label} {value} {helper} />
	{#if signature}
		<div class="flex flex-col gap-1.5">
			<span class="text-label text-foreground">Verify each request</span>
			<code class="rounded-md bg-secondary px-3 py-2 text-mono-id font-mono text-foreground">
				PayRouters-Signature: t=…,v1=…
			</code>
			<p class="text-helper text-muted-foreground">
				Compare v1 with an HMAC-SHA256 of t and the raw body, using this secret.
			</p>
		</div>
	{/if}
	{#snippet footer()}
		<Button class="w-full sm:w-auto" onclick={() => (open = false)}>{doneLabel}</Button>
	{/snippet}
</ResponsiveDialog>
