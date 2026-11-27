<script lang="ts">
	// Gross, fee and net for a paid payment. Before settlement there is nothing to show.
	import type { AdminPayment, CashIn } from '#lib/api/types';
	import { formatRatioFixed } from '#lib/admin-view';
	import { formatIdr } from '#lib/format';

	let { payment, cashIn }: { payment: AdminPayment; cashIn: CashIn | null } = $props();

	const gross = $derived(cashIn?.gross_amount ?? payment.amount);
	const fee = $derived(cashIn?.fee_amount ?? payment.fee_amount);
	const net = $derived(cashIn?.net_amount ?? payment.net_amount);
	const settled = $derived(fee !== null && net !== null);
	const feeShare = $derived(gross > 0 && fee !== null ? fee / gross : null);
</script>

<section
	class="flex flex-col gap-4 rounded-lg border border-border bg-card p-card"
	data-testid="cash-in-ledger"
>
	<h2 class="text-card-title">Cash-in ledger</h2>
	{#if settled}
		<dl class="flex flex-col gap-3 text-body">
			<div class="flex items-center justify-between gap-4">
				<dt class="text-muted-foreground">Gross</dt>
				<dd class="tabular whitespace-nowrap">{formatIdr(gross)}</dd>
			</div>
			<div class="flex items-start justify-between gap-4">
				<dt class="flex flex-col">
					<span class="text-muted-foreground">Fee</span>
					{#if feeShare !== null}
						<span class="text-helper text-muted-foreground">
							{formatRatioFixed(feeShare)} of gross
						</span>
					{/if}
				</dt>
				<dd class="tabular whitespace-nowrap">−{formatIdr(fee)}</dd>
			</div>
		</dl>
		<div
			class="flex flex-col gap-1 border-t border-border pt-4 sm:flex-row sm:items-center sm:justify-between sm:gap-4"
		>
			<span class="text-label">Net received</span>
			<div class="amount-box flex-1 text-amount-lg">
				<span
					class="tabular block text-right whitespace-nowrap"
					class:amount-fit={formatIdr(net).length > 12}
				>
					{formatIdr(net)}
				</span>
			</div>
		</div>
	{:else}
		<p class="text-body text-muted-foreground">
			Not settled yet. Cash-in is recorded when the payment is paid.
		</p>
	{/if}
</section>
