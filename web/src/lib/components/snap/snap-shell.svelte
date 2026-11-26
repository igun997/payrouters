<script lang="ts">
	/*
	 * Customer page frame. Below 640: full-bleed app bar (back, merchant, Sandbox badge).
	 * From 640: slim brand strip over a centred 480 column; the merchant moves above the content.
	 * `livemode` null (unknown, e.g. not found) hides the Sandbox badge and the merchant name.
	 * Without a merchant the app bar shows the PayRouters brand, or a placeholder while loading.
	 */
	import type { Snippet } from 'svelte';
	import ArrowLeft from '@lucide/svelte/icons/arrow-left';
	import Lock from '@lucide/svelte/icons/lock';
	import BrandMark from '#lib/components/app/brand-mark.svelte';
	import Pill from '#lib/components/app/pill.svelte';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import { navigateTo } from '#lib/url';

	let {
		merchant = null,
		livemode = null,
		backUrl = null,
		back = true,
		loading = false,
		children
	}: {
		merchant?: string | null;
		livemode?: boolean | null;
		/** Merchant return URL. When null the back control goes to the previous page. */
		backUrl?: string | null;
		back?: boolean;
		/** True while the payment is being fetched: the app bar shows a name placeholder. */
		loading?: boolean;
		children: Snippet;
	} = $props();

	const sandbox = $derived(livemode === false);
</script>

<div class="flex min-h-svh flex-col bg-background" data-testid="snap-shell">
	<header
		class="flex min-h-14 shrink-0 items-center gap-3 border-b border-border bg-card px-page pt-[env(safe-area-inset-top)] sm:hidden"
	>
		{#if back}
			<button
				type="button"
				aria-label={merchant && backUrl ? `Back to ${merchant}` : 'Go back'}
				onclick={() => (backUrl ? navigateTo(backUrl) : history.back())}
				class="flex size-control shrink-0 items-center justify-center rounded-full border border-border bg-card text-foreground focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
			>
				<ArrowLeft class="size-5" aria-hidden="true" />
			</button>
		{/if}
		{#if merchant}
			<span
				class="min-w-0 flex-1 truncate text-card-title font-semibold"
				data-testid="snap-merchant-bar"
			>
				{merchant}
			</span>
		{:else if loading}
			<Skeleton class="h-4 w-28" />
		{:else}
			<span class="flex min-w-0 flex-1 items-center gap-2 text-card-title font-bold">
				<BrandMark tile size={18} class="text-primary-foreground" />
				PayRouters
			</span>
		{/if}
		{#if sandbox}<Pill tone="sandbox">Sandbox</Pill>{/if}
	</header>

	<header
		class="hidden h-14 shrink-0 items-center justify-between border-b border-border bg-card px-6 sm:flex"
	>
		<div class="flex items-center gap-2.5">
			<BrandMark tile size={18} class="text-primary-foreground" />
			<span class="text-card-title font-bold">PayRouters</span>
		</div>
		{#if sandbox}<Pill tone="sandbox">Sandbox</Pill>{/if}
	</header>

	<main
		class="mx-auto flex w-full flex-1 flex-col gap-section px-page pt-5 pb-6 sm:max-w-[480px] sm:pt-5 sm:pb-4"
	>
		{#if merchant}
			<p class="hidden text-label text-muted-foreground sm:block" data-testid="snap-merchant">
				{merchant}
			</p>
		{/if}
		{@render children()}
	</main>

	<footer
		class="flex items-center justify-center gap-1.5 px-page pb-6 text-helper text-muted-foreground sm:pb-4"
	>
		<Lock class="size-3.5" aria-hidden="true" />
		<span>Secured by</span>
		<BrandMark size={14} class="text-foreground" />
		<span class="font-semibold text-foreground">PayRouters</span>
	</footer>
</div>
