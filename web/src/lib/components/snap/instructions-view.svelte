<script lang="ts">
	/*
	 * A pending payment with a method chosen: amount, deadline and the instructions for that method.
	 * Chooses the variant from `instructions.type` (plus the Mandiri biller code and the IDRX breakdown).
	 */
	import AmountBlock from './amount-block.svelte';
	import BreakdownCard from './breakdown-card.svelte';
	import CopyAction from './copy-action.svelte';
	import Countdown from './countdown.svelte';
	import MandiriBill from './mandiri-bill.svelte';
	import OrderMeta from './order-meta.svelte';
	import PayBefore from './pay-before.svelte';
	import QrisInstructions from './qris-instructions.svelte';
	import RedirectPanel from './redirect-panel.svelte';
	import SimulatorBox from './simulator-box.svelte';
	import StatusLine from './status-line.svelte';
	import VaInstructions from './va-instructions.svelte';
	import { PROVIDER_LABEL, formatIdr } from '#lib/format';
	import type { Instructions, SimulateOutcome, SnapPayment } from '#lib/api/types';

	let {
		payment,
		instructions,
		allowSimulate,
		simulating,
		onsimulate
	}: {
		payment: SnapPayment;
		instructions: Instructions;
		allowSimulate: boolean;
		simulating: boolean;
		onsimulate: (outcome: SimulateOutcome) => void;
	} = $props();

	const total = $derived(payment.payable_amount ?? payment.amount);
	// IDRX: the provider adds a fee on top of the minted amount, so the customer pays more than `amount`.
	const breakdown = $derived(
		payment.payable_amount != null && payment.payable_amount > payment.amount
	);
	const providerName = $derived(PROVIDER_LABEL[payment.provider] ?? payment.provider);
</script>

{#if instructions.type === 'redirect'}
	<div class="flex flex-col gap-section" data-testid="snap-instructions">
		<AmountBlock amount={total} size="lg" />
		<OrderMeta orderId={payment.order_id} description={payment.description} />
		<PayBefore expiresAt={payment.expires_at} />
		{#if payment.items.length > 0}
			<BreakdownCard
				title="Order summary"
				rows={payment.items.map((item) => ({
					label: `${item.name} × ${item.quantity}`,
					value: formatIdr(item.price * item.quantity)
				}))}
				total={{ label: 'Total', value: formatIdr(payment.amount) }}
			/>
		{/if}
		<RedirectPanel url={instructions.url} providerLabel={providerName} />
	</div>
{:else}
	<div class="flex flex-col gap-section" data-testid="snap-instructions">
		<AmountBlock amount={total} size="lg">
			{#snippet actions()}
				{#if instructions.type === 'virtual_account'}
					<CopyAction
						text={String(total)}
						label="Copy amount"
						toastMessage="Amount copied"
						testid="snap-copy-amount"
					/>
				{/if}
			{/snippet}
		</AmountBlock>

		<Countdown
			expiresAt={payment.expires_at}
			label={instructions.type === 'qris' ? 'Scan within' : 'Pay within'}
		/>

		{#if instructions.type === 'qris'}
			<QrisInstructions {payment} {instructions} />
		{:else if instructions.biller_code}
			<MandiriBill {total} {instructions} />
		{:else}
			<VaInstructions {payment} {instructions} {breakdown} />
		{/if}

		{#if allowSimulate}
			<SimulatorBox busy={simulating} {onsimulate} />
		{/if}
	</div>

	<StatusLine title="Waiting for payment" hint="Updates automatically" />
{/if}
