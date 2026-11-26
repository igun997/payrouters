<script lang="ts">
	// Order amount, paid or created time, and the payment's facts. `all` is one card (tablet and
	// desktop). Phones split it: `amount` is the amount card, `details` the facts as label/value rows.
	import type { AdminPayment } from '#lib/api/types';
	import { fullDateTime } from '#lib/admin-view';
	import { formatIdr } from '#lib/format';

	let {
		payment,
		providerLabel,
		methodLabel,
		part = 'all'
	}: {
		payment: AdminPayment;
		providerLabel: string;
		methodLabel: string | null;
		part?: 'all' | 'amount' | 'details';
	} = $props();

	const total = $derived(payment.payable_amount ?? payment.amount);
	const facts = $derived([
		{ label: 'Product', value: payment.product.name, mono: false },
		{ label: 'Provider', value: providerLabel, mono: false },
		{ label: 'Method', value: methodLabel ?? 'Not chosen yet', mono: false },
		{
			label: 'Checkout',
			value: payment.checkout === 'snap' ? 'Snap' : 'Provider page',
			mono: false
		},
		{ label: 'Provider reference', value: payment.provider_ref ?? '—', mono: true },
		{ label: 'Expires', value: fullDateTime(payment.expires_at), mono: false },
		{ label: 'Customer', value: payment.customer?.name ?? '—', mono: false },
		{ label: 'Email', value: payment.customer?.email ?? '—', mono: false },
		{ label: 'Description', value: payment.description ?? '—', mono: false }
	]);
</script>

{#snippet amountBlock()}
	<div class="flex flex-col gap-1">
		<p class="text-label text-muted-foreground">Order amount</p>
		<p class="tabular text-amount-lg whitespace-nowrap">{formatIdr(payment.amount)}</p>
		{#if payment.status === 'paid'}
			<p class="text-helper text-muted-foreground">Paid on {fullDateTime(payment.paid_at)}</p>
		{:else}
			<p class="text-helper text-muted-foreground">Created {fullDateTime(payment.created_at)}</p>
		{/if}
		{#if total !== payment.amount}
			<p class="text-label">
				Total to pay <span class="tabular whitespace-nowrap">{formatIdr(total)}</span>
			</p>
		{/if}
	</div>
{/snippet}

{#if part === 'amount'}
	<section
		class="flex flex-col gap-4 rounded-lg border border-border bg-card p-card"
		data-testid="payment-summary"
	>
		{@render amountBlock()}
	</section>
{:else if part === 'details'}
	<section
		class="flex flex-col gap-3 rounded-lg border border-border bg-card p-card"
		data-testid="payment-details"
	>
		<h2 class="text-card-title">Details</h2>
		<dl class="flex flex-col gap-3">
			{#each facts as fact (fact.label)}
				<div class="flex items-start justify-between gap-4">
					<dt class="shrink-0 text-helper text-muted-foreground">{fact.label}</dt>
					<dd
						class={fact.mono
							? 'min-w-0 text-right text-mono-id font-mono break-all'
							: 'min-w-0 text-right text-label break-words'}
					>
						{fact.value}
					</dd>
				</div>
			{/each}
		</dl>
	</section>
{:else}
	<section
		class="flex flex-col gap-5 rounded-lg border border-border bg-card p-card"
		data-testid="payment-summary"
	>
		{@render amountBlock()}
		<dl class="grid grid-cols-2 gap-x-6 gap-y-4 border-t border-border pt-5 md:grid-cols-3">
			{#each facts as fact (fact.label)}
				<div class="flex min-w-0 flex-col gap-0.5">
					<dt class="text-helper text-muted-foreground">{fact.label}</dt>
					<dd class={fact.mono ? 'text-mono-id font-mono break-all' : 'text-label break-words'}>
						{fact.value}
					</dd>
				</div>
			{/each}
		</dl>
	</section>
{/if}
