<script lang="ts">
	// What the customer has to do: transfer to a virtual account, scan a QRIS code, or open the
	// provider-hosted page. Shows a neutral line until the customer has chosen a method.
	import type { AdminPayment } from '#lib/api/types';
	import { Button } from '#lib/components/ui/button';
	import CopyButton from './copy-button.svelte';
	import CopyField from './copy-field.svelte';
	import QrCode from './qr-code.svelte';
	import { fullDateTime, groupDigits } from '#lib/admin-view';
	import { formatIdr } from '#lib/format';
	import { openExternal } from '#lib/url';
	import ExternalLink from '@lucide/svelte/icons/external-link';

	let {
		payment,
		methodLabel,
		providerLabel
	}: { payment: AdminPayment; methodLabel: string | null; providerLabel: string } = $props();

	const ins = $derived(payment.instructions);
	const total = $derived(payment.payable_amount ?? payment.amount);
</script>

<section
	class="flex flex-col gap-4 rounded-lg border border-border bg-card p-card"
	data-testid="payment-instructions"
>
	<h2 class="text-card-title">Payment instructions</h2>
	{#if !ins}
		<p class="text-body text-muted-foreground">
			No payment instructions yet. The customer has not chosen a method.
		</p>
	{:else if ins.type === 'virtual_account'}
		<div class="flex items-center gap-3">
			<span
				class="flex size-10 shrink-0 items-center justify-center rounded-md border border-border bg-background font-mono text-helper font-semibold"
			>
				{ins.bank.toUpperCase()}
			</span>
			<div class="flex min-w-0 flex-col">
				<span class="text-label">{methodLabel ?? `${ins.bank.toUpperCase()} Virtual Account`}</span>
				<span class="text-micro text-muted-foreground uppercase">
					{ins.account_name ?? providerLabel}
				</span>
			</div>
		</div>
		{#if ins.account_number}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<span class="tabular text-mono-va whitespace-nowrap" data-testid="va-number">
					{groupDigits(ins.account_number)}
				</span>
				<CopyButton text={ins.account_number} label="Copy number" />
			</div>
		{/if}
		{#if ins.biller_code}
			<CopyField label="Biller code" value={ins.biller_code} />
		{/if}
		{#if ins.bill_key}
			<CopyField label="Bill key" value={ins.bill_key} />
		{/if}
		<p class="text-helper text-muted-foreground">
			Customer pays before {fullDateTime(payment.expires_at)}. Amount must match exactly: {formatIdr(
				total
			)}.
		</p>
	{:else if ins.type === 'qris'}
		<div class="flex flex-col items-start gap-4 sm:flex-row">
			{#if ins.qr_string}
				<QrCode
					text={ins.qr_string}
					size={200}
					alt="QRIS code for this payment"
					class="w-44 shrink-0"
				/>
			{:else if ins.qr_image_url}
				<!-- Some providers return only an image for the QR code. -->
				<img
					src={ins.qr_image_url}
					alt="QRIS code for this payment"
					width="176"
					height="176"
					class="h-auto w-44 shrink-0 rounded-md bg-white p-2"
				/>
			{/if}
			<div class="flex min-w-0 flex-1 flex-col gap-3">
				<span class="text-label">{methodLabel ?? 'QRIS'}</span>
				{#if ins.qr_string}
					<CopyField label="QR string" value={ins.qr_string} />
				{/if}
				{#if ins.qr_image_url}
					<Button
						variant="link"
						size="sm"
						class="w-fit px-0"
						onclick={() => openExternal(ins.qr_image_url)}
					>
						Open provider QR image
					</Button>
				{/if}
			</div>
		</div>
	{:else}
		<div class="flex flex-col gap-3">
			<span class="text-label">Provider-hosted page</span>
			<CopyField label="Redirect URL" value={ins.url} />
			<Button variant="outline" size="sm" class="w-fit" onclick={() => openExternal(ins.url)}>
				<ExternalLink data-icon="inline-start" />
				Open provider page
			</Button>
		</div>
	{/if}
</section>
