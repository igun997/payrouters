<script lang="ts">
	// One chronological list: payment created, provider notifications, cash-in recorded and outbound
	// webhook deliveries. Failed deliveries offer a retry.
	import { Button } from '#lib/components/ui/button';
	import type { AdminPayment, CashIn, Delivery, PaymentEvent } from '#lib/api/types';
	import { fullDateTime } from '#lib/admin-view';
	import { formatIdr } from '#lib/format';
	import { cn } from '#lib/utils';

	let {
		payment,
		events,
		deliveries,
		cashIn,
		busy = null,
		onretry
	}: {
		payment: AdminPayment;
		events: PaymentEvent[];
		deliveries: Delivery[];
		cashIn: CashIn | null;
		/** Key of the action in flight (e.g. `retry:<id>`), or null. */
		busy?: string | null;
		onretry: (delivery: Delivery) => void;
	} = $props();

	type Item = {
		key: string;
		at: string;
		title: string;
		detail?: string;
		tone: 'done' | 'neutral' | 'failed';
		delivery?: Delivery;
	};

	const items = $derived.by<Item[]>(() => {
		const out: Item[] = [
			{
				key: 'created',
				at: payment.created_at,
				title: 'Payment created',
				detail: `Checkout ${payment.checkout === 'snap' ? 'Snap' : 'provider page'} · ${payment.product.name}`,
				tone: 'neutral'
			}
		];
		for (const event of events) {
			out.push({
				key: `event:${event.id}`,
				at: event.received_at,
				title: `Provider notification: ${event.outcome}`,
				detail: [event.verified ? 'Signature verified' : 'Not verified', event.error]
					.filter(Boolean)
					.join(' · '),
				tone:
					event.outcome === 'applied'
						? 'done'
						: event.outcome === 'rejected' || event.outcome === 'error'
							? 'failed'
							: 'neutral'
			});
		}
		if (cashIn) {
			out.push({
				key: 'cash-in',
				at: cashIn.paid_at,
				title: 'Cash-in recorded',
				detail: `Gross ${formatIdr(cashIn.gross_amount)} · fee ${formatIdr(cashIn.fee_amount)} · net ${formatIdr(cashIn.net_amount)}`,
				tone: 'done'
			});
		}
		for (const delivery of deliveries) {
			const at = delivery.delivered_at ?? delivery.next_attempt_at ?? delivery.created_at;
			if (delivery.status === 'delivered') {
				out.push({
					key: `delivery:${delivery.id}`,
					at,
					title: `Webhook delivered to ${payment.product.name}`,
					detail: `${delivery.event_type} · HTTP ${delivery.last_status_code ?? 200}`,
					tone: 'done',
					delivery
				});
			} else if (delivery.status === 'failed') {
				out.push({
					key: `delivery:${delivery.id}`,
					at,
					title: 'Webhook failed',
					detail: delivery.last_error ?? `${delivery.attempts} attempts`,
					tone: 'failed',
					delivery
				});
			} else {
				out.push({
					key: `delivery:${delivery.id}`,
					at,
					title: 'Webhook retry scheduled',
					detail: `${delivery.event_type} · attempt ${delivery.attempts + 1}`,
					tone: 'neutral',
					delivery
				});
			}
		}
		return out.sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
	});

	function when(iso: string): string {
		return fullDateTime(iso);
	}
</script>

<section
	class="flex flex-col gap-4 rounded-lg border border-border bg-card p-card"
	data-testid="activity-timeline"
>
	<h2 class="text-card-title">Activity</h2>
	<ol class="flex flex-col">
		{#each items as item, index (item.key)}
			<li class="relative flex gap-4 pb-5 last:pb-0" data-testid="activity-item">
				{#if index < items.length - 1}
					<span class="absolute top-4 bottom-0 left-[7px] w-px bg-border" aria-hidden="true"></span>
				{/if}
				<span
					class={cn(
						'relative mt-1 size-[15px] shrink-0 rounded-full border-2',
						item.tone === 'done' && 'border-signal bg-signal',
						item.tone === 'failed' && 'border-failed-foreground bg-failed-foreground',
						item.tone === 'neutral' && 'border-input bg-card'
					)}
					aria-hidden="true"
				></span>
				<div class="flex min-w-0 flex-1 flex-col gap-0.5">
					<div class="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5">
						<span class="text-label">{item.title}</span>
						<span class="tabular text-helper whitespace-nowrap text-muted-foreground">
							{when(item.at)}
						</span>
					</div>
					{#if item.detail}
						<p class="text-helper break-words text-muted-foreground">{item.detail}</p>
					{/if}
					{#if item.delivery && item.delivery.status !== 'delivered'}
						<div class="mt-1">
							<Button
								variant="outline"
								size="sm"
								disabled={busy !== null}
								onclick={() => item.delivery && onretry(item.delivery)}
							>
								Retry delivery
							</Button>
						</div>
					{/if}
				</div>
			</li>
		{:else}
			<li class="text-body text-muted-foreground">No activity yet.</li>
		{/each}
	</ol>
</section>
