<script lang="ts">
	/*
	 * Provider-hosted checkout: explains the hop, offers a button and moves the customer on after
	 * two seconds. The page keeps polling, so it updates when the payment is received.
	 */
	import ExternalLink from '@lucide/svelte/icons/external-link';
	import { Button } from '#lib/components/ui/button';
	import { navigateTo } from '#lib/url';

	const SECONDS = 2;

	let { url, providerLabel }: { url: string; providerLabel: string } = $props();

	let left = $state(SECONDS);

	$effect(() => {
		if (left <= 0) {
			navigateTo(url);
			return;
		}
		const timer = setTimeout(() => (left -= 1), 1000);
		return () => clearTimeout(timer);
	});

	const elapsed = $derived(((SECONDS - left) / SECONDS) * 100);
</script>

<section
	class="flex flex-col gap-4 rounded-lg border border-border bg-card p-card"
	data-testid="snap-redirect"
>
	<div class="flex flex-col gap-1">
		<h2 class="text-card-title font-semibold">You will finish this payment on {providerLabel}</h2>
		<p class="text-body text-muted-foreground">
			{providerLabel} shows the bank transfer and e-wallet options on its secure page.
		</p>
	</div>
	<Button size="lg" class="w-full" onclick={() => navigateTo(url)}>
		<ExternalLink data-icon="inline-start" aria-hidden="true" />
		Continue to {providerLabel}
	</Button>
	<div class="flex flex-col gap-2">
		<p class="text-helper text-muted-foreground" role="status">
			Taking you to {providerLabel} in {left}
			{left === 1 ? 'second' : 'seconds'}
		</p>
		<div class="h-1 overflow-hidden rounded-full bg-secondary" aria-hidden="true">
			<div
				class="h-full rounded-full bg-primary transition-[width] duration-1000 ease-linear"
				style="width: {elapsed}%"
			></div>
		</div>
	</div>
	<p class="text-helper text-muted-foreground">
		This page updates automatically when your payment is received.
	</p>
</section>
