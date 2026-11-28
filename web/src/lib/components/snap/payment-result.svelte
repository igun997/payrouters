<script lang="ts">
	/* Terminal payment (paid, expired, failed, cancelled): outcome, receipt and way back to the merchant. */
	import Banner from '#lib/components/app/banner.svelte';
	import BreakdownCard, { type BreakdownRow } from './breakdown-card.svelte';
	import ResultHero from './result-hero.svelte';
	import ReturnAction from './return-action.svelte';
	import { METHOD_LABEL } from './method-meta';
	import type { Snippet } from 'svelte';
	import type { SnapPayment } from '#lib/api/types';
	import { PROVIDER_LABEL, formatDateTime, formatIdr } from '#lib/format';
	import { safeHttpUrl } from '#lib/url';

	let {
		payment,
		merchantName,
		restartable = false,
		actions
	}: {
		payment: SnapPayment;
		merchantName: string;
		/** The payment can be started over: the customer is not sent back to the merchant on a timer. */
		restartable?: boolean;
		/** Extra actions under the outcome, e.g. starting an expired payment over. */
		actions?: Snippet;
	} = $props();

	const paid = $derived(payment.status === 'paid');
	// Design: receipt rows for paid and failed; cancelled and expired show the outcome only.
	const showBreakdown = $derived(payment.status === 'paid' || payment.status === 'failed');
	const total = $derived(payment.payable_amount ?? payment.amount);
	const providerName = $derived(PROVIDER_LABEL[payment.provider] ?? payment.provider);
	const returnUrl = $derived(safeHttpUrl(payment.return_url));

	const outcome = $derived.by(() => {
		switch (payment.status) {
			case 'paid':
				return {
					tone: 'success' as const,
					title: 'Payment received',
					detail: `Paid on ${formatDateTime(payment.paid_at)}`
				};
			case 'expired':
				return {
					tone: 'expired' as const,
					title: 'Payment expired',
					detail: `Order ${payment.order_id} was not paid within the time limit. Nothing was charged.`
				};
			case 'failed':
				return {
					tone: 'failed' as const,
					title: 'Payment failed',
					detail: `${providerName} declined this payment. No money was taken.`
				};
			default:
				return {
					tone: 'cancelled' as const,
					title: 'Payment cancelled',
					detail: `Order ${payment.order_id} was cancelled. Nothing was charged.`
				};
		}
	});

	const rows: BreakdownRow[] = $derived([
		...(paid ? [{ label: 'Merchant', value: merchantName }] : []),
		{ label: 'Order', value: payment.order_id, mono: true },
		...(payment.method ? [{ label: 'Method', value: METHOD_LABEL[payment.method] }] : [])
	]);
</script>

<div class="flex flex-col gap-section" data-testid="snap-result">
	<ResultHero
		tone={outcome.tone}
		title={outcome.title}
		amount={paid ? formatIdr(total) : null}
		detail={outcome.detail}
		status={payment.status}
	/>

	{#if showBreakdown}
		<BreakdownCard
			{rows}
			total={{ label: paid ? 'Amount paid' : 'Amount', value: formatIdr(total) }}
		/>
	{/if}

	{@render actions?.()}

	{#if payment.status === 'expired'}
		<Banner tone="info" title="Already transferred?">
			Banks can take a few minutes to confirm. Keep your transfer receipt and send the order ID {payment.order_id}
			to {merchantName}.
		</Banner>
	{/if}

	{#if returnUrl}
		<ReturnAction url={returnUrl} label={`Back to ${merchantName}`} auto={!restartable} />
	{/if}
</div>
