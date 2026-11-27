<script lang="ts" module>
	/** Desktop columns (md and up) shared by the outbound table header and its rows. */
	export const DELIVERY_COLUMNS =
		'md:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)_minmax(0,0.9fr)_minmax(0,1.4fr)_132px] xl:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)_110px_80px_minmax(0,1.2fr)_minmax(0,1.1fr)_132px]';

	/** Attempts the server makes before a delivery is marked failed (the retry schedule has 8 steps). */
	export const MAX_ATTEMPTS = 8;
</script>

<script lang="ts">
	import { resolve } from '$app/paths';
	import type { Delivery, DeliveryStatus } from '#lib/api/types';
	import Pill from '#lib/components/app/pill.svelte';
	import JsonBlock from '#lib/components/app/json-block.svelte';
	import { Button } from '#lib/components/ui/button';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import RotateCw from '@lucide/svelte/icons/rotate-cw';
	import { formatDateTime } from '#lib/format';
	import { cn } from '#lib/utils';

	/**
	 * One outbound delivery to a product's webhook URL. A card below md (status and response time
	 * first), a table row from md. Retry is enabled for pending and failed deliveries and disabled
	 * once delivered.
	 */
	let {
		delivery,
		productName,
		targetUrl,
		expanded,
		retrying,
		ontoggle,
		onretry
	}: {
		delivery: Delivery;
		productName: string;
		targetUrl: string | null;
		expanded: boolean;
		retrying: boolean;
		ontoggle: () => void;
		onretry: () => void;
	} = $props();

	const phone = new IsMobile(768);

	const statuses: Record<
		DeliveryStatus,
		{ label: string; tone: 'success' | 'pending' | 'failed' }
	> = {
		delivered: { label: 'Delivered', tone: 'success' },
		pending: { label: 'Pending', tone: 'pending' },
		failed: { label: 'Failed', tone: 'failed' }
	};
	const status = $derived(statuses[delivery.status]);
	const delivered = $derived(delivery.status === 'delivered');

	/** Next attempt, or the moment of delivery once delivered. */
	const nextValue = $derived(
		delivery.next_attempt_at ? formatDateTime(delivery.next_attempt_at) : 'None, retry manually'
	);
	const responseNote = $derived(
		delivered ? `Delivered ${formatDateTime(delivery.delivered_at)}` : `Next ${nextValue}`
	);
</script>

{#snippet detail()}
	<div
		data-testid="delivery-detail"
		class="flex flex-col gap-4 border-t pt-4 md:col-span-full md:grid md:grid-cols-[minmax(0,1fr)_300px] md:gap-6 md:pb-4"
	>
		<div class="flex min-w-0 flex-col gap-2">
			<span class="text-label">Payload</span>
			<JsonBlock value={delivery.payload} />
		</div>
		<div class="flex min-w-0 flex-col gap-4 rounded-md border p-4">
			<span class="text-label">Delivery</span>
			<dl class="flex flex-col gap-3 text-cell">
				<div class="flex justify-between gap-3">
					<dt class="text-muted-foreground">Attempts</dt>
					<dd class="tabular">{delivery.attempts}/{MAX_ATTEMPTS}</dd>
				</div>
				<div class="flex justify-between gap-3">
					<dt class="text-muted-foreground">Last status</dt>
					<dd class="tabular">
						{delivery.last_status_code ? `HTTP ${delivery.last_status_code}` : '—'}
					</dd>
				</div>
				{#if delivery.last_error}
					<div class="flex flex-col gap-1">
						<dt class="text-muted-foreground">Last error</dt>
						<dd class="text-failed-foreground">{delivery.last_error}</dd>
						{#if targetUrl}
							<dd class="font-mono text-helper break-all text-muted-foreground">{targetUrl}</dd>
						{/if}
					</div>
				{/if}
				<div class="flex justify-between gap-3">
					<dt class="text-muted-foreground">Next attempt</dt>
					<dd class="text-right">{delivered ? '—' : nextValue}</dd>
				</div>
				<div class="flex flex-col gap-1">
					<dt class="text-muted-foreground">Event ID</dt>
					<dd class="text-mono-id font-mono break-all">{delivery.event_id}</dd>
				</div>
			</dl>
			<Button
				variant="outline"
				class="w-full"
				onclick={onretry}
				disabled={delivered || retrying}
				data-testid="retry-delivery-detail"
			>
				<RotateCw data-icon="inline-start" />
				Retry delivery
			</Button>
		</div>
	</div>
{/snippet}

<li
	data-testid="delivery-row"
	data-status={delivery.status}
	class={cn(
		'flex flex-col gap-3 p-card md:grid md:items-center md:gap-x-4 md:px-6 md:py-3',
		DELIVERY_COLUMNS
	)}
>
	{#if phone.current}
		<div class="flex items-center justify-between gap-3">
			<span data-testid="delivery-status" data-status={delivery.status}>
				<Pill tone={status.tone}>{status.label}</Pill>
			</span>
			<span class="tabular text-right text-helper text-muted-foreground">{responseNote}</span>
		</div>

		<div class="flex min-w-0 flex-col gap-1">
			<span class="min-w-0 text-mono-id font-mono break-all">{delivery.event_type}</span>
			<a
				href={resolve('/(admin)/payments/[id]', { id: delivery.payment_id })}
				class="inline-flex min-h-touch w-fit items-center font-mono text-helper text-muted-foreground underline-offset-4 hover:underline"
			>
				payment {delivery.payment_id.slice(0, 8)}…
			</a>
			<span class="min-w-0 text-helper break-words text-muted-foreground">
				{productName} · {delivery.attempts}/{MAX_ATTEMPTS} attempts ·
				{delivery.last_status_code ? `HTTP ${delivery.last_status_code}` : 'no response'}
			</span>
			{#if delivery.last_error}
				<span class="text-helper break-words text-failed-foreground">{delivery.last_error}</span>
			{/if}
		</div>

		<div class="flex items-center gap-2">
			<Button
				variant="outline"
				class="min-h-touch flex-1"
				onclick={onretry}
				disabled={delivered || retrying}
				data-testid="retry-delivery"
			>
				<RotateCw data-icon="inline-start" />
				Retry
			</Button>
			<button
				type="button"
				onclick={ontoggle}
				aria-expanded={expanded}
				class="inline-flex min-h-touch items-center gap-1.5 rounded-md px-2 text-label text-foreground hover:bg-muted"
			>
				{#if expanded}<ChevronDown class="size-4" />{:else}<ChevronRight class="size-4" />{/if}
				{expanded ? 'Hide details' : 'Show details'}
			</button>
		</div>

		{#if expanded}
			{@render detail()}
		{/if}
	{:else}
		<div class="flex min-w-0 flex-col gap-0.5">
			<span class="min-w-0 text-mono-id font-mono break-all">{delivery.event_type}</span>
			<a
				href={resolve('/(admin)/payments/[id]', { id: delivery.payment_id })}
				class="inline-flex min-h-touch w-fit items-center font-mono text-helper text-muted-foreground underline-offset-4 hover:underline"
			>
				payment {delivery.payment_id.slice(0, 8)}…
			</a>
		</div>

		<span class="min-w-0 text-cell break-words" title={productName}>{productName}</span>

		<div class="flex flex-col items-start gap-1.5">
			<span data-testid="delivery-status" data-status={delivery.status}>
				<Pill tone={status.tone}>{status.label}</Pill>
			</span>
			<span class="tabular text-helper whitespace-nowrap text-muted-foreground xl:hidden">
				{delivery.attempts}/{MAX_ATTEMPTS} attempts
			</span>
		</div>

		<span class="tabular hidden text-cell xl:block">{delivery.attempts}/{MAX_ATTEMPTS}</span>

		<div class="flex min-w-0 flex-col gap-0.5 text-cell">
			<span class="tabular">
				{delivery.last_status_code ? `HTTP ${delivery.last_status_code}` : '—'}
			</span>
			{#if delivery.last_error}
				<span class="truncate text-helper text-failed-foreground" title={delivery.last_error}>
					{delivery.last_error}
				</span>
			{/if}
			<span class="text-helper text-muted-foreground xl:hidden">{responseNote}</span>
		</div>

		<span class="tabular hidden min-w-0 text-helper break-words text-muted-foreground xl:block">
			{delivered ? `Delivered ${formatDateTime(delivery.delivered_at)}` : nextValue}
		</span>

		<div class="flex items-center gap-2 md:justify-end">
			<Button
				variant="outline"
				size="sm"
				onclick={onretry}
				disabled={delivered || retrying}
				data-testid="retry-delivery"
			>
				<RotateCw data-icon="inline-start" />
				Retry
			</Button>
			<button
				type="button"
				onclick={ontoggle}
				aria-expanded={expanded}
				class="inline-flex min-h-touch items-center gap-1.5 rounded-md px-2 text-label text-foreground hover:bg-muted md:h-9 md:w-9 md:justify-center md:px-0"
			>
				{#if expanded}<ChevronDown class="size-4" />{:else}<ChevronRight class="size-4" />{/if}
				<span class="md:sr-only">{expanded ? 'Hide details' : 'Show details'}</span>
			</button>
		</div>

		{#if expanded}
			{@render detail()}
		{/if}
	{/if}
</li>
