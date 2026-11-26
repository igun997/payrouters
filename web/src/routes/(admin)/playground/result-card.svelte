<script lang="ts">
	import { resolve } from '$app/paths';
	import type { AdminPayment } from '#lib/api/types';
	import StatusBadge from '#lib/components/app/status-badge.svelte';
	import CopyField from '#lib/components/app/copy-field.svelte';
	import JsonBlock from '#lib/components/app/json-block.svelte';
	import { Button } from '#lib/components/ui/button';
	import ExternalLink from '@lucide/svelte/icons/external-link';
	import { formatDateTime, formatIdr } from '#lib/format';
	import { safeHttpUrl } from '#lib/url';

	/**
	 * Summary of the payment the playground just created: status, the customer page link and the
	 * raw response as the API returned it.
	 */
	let {
		payment,
		providerName,
		methodLabel
	}: { payment: AdminPayment; providerName: string; methodLabel: string } = $props();

	const snapHref = $derived(safeHttpUrl(payment.snap_url));
	const shortId = $derived(`${payment.id.slice(0, 8)}…`);
</script>

<section
	class="flex flex-col gap-5 rounded-lg border bg-card p-card"
	data-testid="payment-result"
	data-status={payment.status}
>
	<div class="flex items-start justify-between gap-3">
		<div class="flex min-w-0 flex-col gap-1">
			<h2 class="text-card-title">Payment created</h2>
			<p class="truncate text-helper text-muted-foreground">
				<span class="font-mono">{payment.order_id}</span> · {providerName}
			</p>
		</div>
		<StatusBadge status={payment.status} />
	</div>

	<dl class="flex flex-col divide-y rounded-md border bg-card text-cell">
		<div class="flex items-center justify-between gap-3 px-4 py-2.5">
			<dt class="text-muted-foreground">Payment ID</dt>
			<dd class="text-mono-id font-mono">{shortId}</dd>
		</div>
		<div class="flex items-center justify-between gap-3 px-4 py-2.5">
			<dt class="text-muted-foreground">Amount</dt>
			<dd class="tabular font-semibold whitespace-nowrap">{formatIdr(payment.amount)}</dd>
		</div>
		<div class="flex items-center justify-between gap-3 px-4 py-2.5">
			<dt class="text-muted-foreground">Expires</dt>
			<dd class="tabular text-right">{formatDateTime(payment.expires_at)}</dd>
		</div>
		<div class="flex items-center justify-between gap-3 px-4 py-2.5">
			<dt class="text-muted-foreground">Method</dt>
			<dd class="text-right">{methodLabel}</dd>
		</div>
	</dl>

	{#if snapHref}
		<div class="flex flex-col gap-3">
			<div class="flex gap-2">
				<Button href={snapHref} target="_blank" rel="noopener noreferrer" class="flex-1">
					<ExternalLink data-icon="inline-start" />
					Open Snap page
				</Button>
			</div>
			<CopyField label="Snap URL" value={snapHref} />
		</div>
	{/if}

	<div class="flex flex-col gap-2">
		<span class="text-label">Raw response</span>
		<JsonBlock value={payment} />
	</div>

	<div class="flex justify-end">
		<Button variant="ghost" size="sm" href={resolve('/(admin)/payments/[id]', { id: payment.id })}>
			Open payment details
		</Button>
	</div>
</section>
