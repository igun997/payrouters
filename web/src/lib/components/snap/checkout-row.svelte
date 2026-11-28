<script lang="ts">
	/* One provider's hosted checkout (the provider's own page) in the choose-how-to-pay list. */
	import { providerMark } from '#lib/admin-view';
	import PayRow from './pay-row.svelte';
	import { chargeAmount, chargeRate } from '#lib/charge';
	import type { Provider, SnapCheckout } from '#lib/api/types';

	let {
		checkout,
		state = 'idle',
		disabled = false,
		onpick
	}: {
		checkout: SnapCheckout;
		state?: 'idle' | 'connecting' | 'error';
		/** Another row is starting; this one cannot be tapped. */
		disabled?: boolean;
		onpick: (provider: Provider) => void;
	} = $props();

	const sub = $derived.by(() => {
		if (state === 'connecting') return `Connecting to ${checkout.label}…`;
		if (state === 'error') return 'Could not start this payment. Try again.';
		if (checkout.quote) {
			return `You pay ${chargeAmount(checkout.quote)} (${chargeRate(checkout.quote)})`;
		}
		return `Pay on ${checkout.label}`;
	});
</script>

<PayRow
	testid={`snap-checkout-${checkout.provider}`}
	{state}
	{disabled}
	title={checkout.label}
	{sub}
	onclick={() => onpick(checkout.provider)}
>
	{#snippet leading()}
		<span
			class="flex size-11 shrink-0 items-center justify-center rounded-md bg-secondary text-label font-semibold text-foreground"
			aria-hidden="true"
		>
			{providerMark(checkout.provider)}
		</span>
	{/snippet}
</PayRow>
