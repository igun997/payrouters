<script lang="ts">
	/*
	 * Provider-hosted checkout: explains the hop, offers a button and moves the customer on after
	 * two seconds, unless `auto` is off (the payer just came back from the provider's page). The page
	 * keeps polling, so it updates when the payment is received. When the provider charges another
	 * currency (`charge`, PayPal's US dollars) the converted amount and rate are shown and the
	 * customer moves on by the button only: two seconds is too short to read what they will pay.
	 */
	import ExternalLink from '@lucide/svelte/icons/external-link';
	import { Button } from '#lib/components/ui/button';
	import { navigateTo } from '#lib/url';
	import { chargeAmount, chargeRate, chargeSource } from '#lib/charge';
	import { formatIdr } from '#lib/format';
	import type { Charge } from '#lib/api/types';

	const SECONDS = 2;

	let {
		url,
		providerLabel,
		amount,
		charge = null,
		auto = true
	}: {
		url: string;
		providerLabel: string;
		/** The order total in rupiah. */
		amount: number;
		charge?: Charge | null;
		auto?: boolean;
	} = $props();

	const automatic = $derived(auto && !charge);

	let left = $state(SECONDS);

	$effect(() => {
		if (!automatic) return;
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
			{providerLabel} shows its payment options on its secure page.
		</p>
	</div>
	{#if charge}
		<div
			class="flex flex-col gap-1 rounded-md bg-secondary p-3"
			data-testid="snap-charge"
			data-currency={charge.currency}
		>
			<span class="text-helper text-muted-foreground">
				{providerLabel} charges in {charge.currency}
			</span>
			<span class="tabular text-card-title font-semibold" data-testid="snap-charge-amount">
				{chargeAmount(charge)}
			</span>
			<span class="text-helper break-words text-muted-foreground">
				{formatIdr(amount)} at {chargeRate(charge)} · {chargeSource(charge)}
			</span>
		</div>
	{/if}
	<Button size="lg" class="w-full" onclick={() => navigateTo(url)}>
		<ExternalLink data-icon="inline-start" aria-hidden="true" />
		Continue to {providerLabel}
	</Button>
	{#if automatic}
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
	{/if}
	<p class="text-helper text-muted-foreground">
		This page updates automatically when your payment is received.
	</p>
</section>
