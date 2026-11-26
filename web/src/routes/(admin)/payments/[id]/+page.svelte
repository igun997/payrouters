<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { toast } from 'svelte-sonner';
	import { adminApi } from '#lib/api/admin';
	import { errorMessage } from '#lib/api/client';
	import type {
		AdminPayment,
		Delivery,
		Meta,
		PaymentDetail,
		SimulateOutcome
	} from '#lib/api/types';
	import { Resource } from '#lib/state/resource.svelte';
	import { loadMeta, methodLabels, providerLabels } from '#lib/state/catalog.svelte';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';
	import { Button } from '#lib/components/ui/button';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import PageHeader from '#lib/components/app/page-header.svelte';
	import StatusBadge from '#lib/components/app/status-badge.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import PaymentSummary from '#lib/components/app/payment-summary.svelte';
	import CashInLedger from '#lib/components/app/cash-in-ledger.svelte';
	import ActivityTimeline from '#lib/components/app/activity-timeline.svelte';
	import PaymentInstructions from '#lib/components/app/payment-instructions.svelte';
	import SnapLink from '#lib/components/app/snap-link.svelte';
	import JsonBlock from '#lib/components/app/json-block.svelte';
	import ArrowLeft from '@lucide/svelte/icons/arrow-left';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import ExternalLink from '@lucide/svelte/icons/external-link';
	import { fullDateTime } from '#lib/admin-view';
	import { PROVIDER_LABEL } from '#lib/format';
	import { openExternal } from '#lib/url';

	const id = $derived(page.params.id ?? '');
	const detail = new Resource<PaymentDetail>();
	const meta = new Resource<Meta>();
	const phone = new IsMobile(768);
	let busy = $state<string | null>(null);

	$effect(() => {
		void meta.load(loadMeta);
	});

	$effect(() => {
		const current = id;
		void detail.load(() => adminApi.getPayment(current));
	});

	const providerName = $derived({ ...PROVIDER_LABEL, ...providerLabels(meta.data) });
	const methodName = $derived(methodLabels(meta.data));

	function reload() {
		const current = id;
		void detail.load(() => adminApi.getPayment(current));
	}

	async function run(key: string, action: () => Promise<unknown>, success: string) {
		busy = key;
		try {
			await action();
			toast.success(success);
			reload();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			busy = null;
		}
	}

	function simulate(outcome: SimulateOutcome) {
		const current = id;
		return run(
			`simulate:${outcome}`,
			() => adminApi.simulatePayment(current, outcome),
			`Simulated ${outcome}`
		);
	}

	function retry(delivery: Delivery) {
		return run(
			`retry:${delivery.id}`,
			() => adminApi.retryDelivery(delivery.id),
			'Delivery queued for retry'
		);
	}

	const p = $derived(detail.data?.payment);
</script>

<PageHeader
	title="Payment detail"
	description={p ? `Created ${fullDateTime(p.created_at)}` : undefined}
/>

<div class="flex flex-col gap-section" data-testid="page-payment-detail">
	{#if !phone.current}
		<a
			href={resolve('/(admin)/payments')}
			class="inline-flex min-h-touch w-fit items-center gap-1.5 text-label text-muted-foreground hover:text-foreground"
		>
			<ArrowLeft class="size-4" />
			Payments
		</a>
	{/if}

	<ErrorAlert error={detail.error} subject="this payment" onretry={reload} />

	{#if (detail.loading && !detail.data) || (meta.loading && !meta.data)}
		<div class="flex flex-col gap-section">
			<Skeleton class="h-10 w-72" />
			<Skeleton class="h-48 w-full" />
			<Skeleton class="h-64 w-full" />
		</div>
	{:else if detail.data && p}
		{@const d = detail.data}

		{#snippet instructions(payment: AdminPayment)}
			<PaymentInstructions
				{payment}
				methodLabel={payment.method ? (methodName[payment.method] ?? payment.method) : null}
				providerLabel={providerName[payment.provider] ?? payment.provider}
			/>
			<SnapLink url={payment.snap_url} expiresAt={payment.expires_at} />
		{/snippet}

		{#snippet simulator(payment: AdminPayment)}
			{#if payment.provider === 'mock' && payment.status === 'pending'}
				<section
					class="flex flex-col gap-3 rounded-lg border border-dashed border-border bg-card p-card"
					data-testid="simulator"
				>
					<h2 class="text-card-title">Sandbox simulator</h2>
					<p class="text-helper text-muted-foreground">
						{#if payment.method}
							Mock provider only. Moves this payment to a terminal status.
						{:else}
							The customer has not chosen a method yet. Paid is available once they do.
						{/if}
					</p>
					<div class="flex flex-wrap gap-2">
						<Button
							size="sm"
							disabled={busy !== null || !payment.method}
							onclick={() => simulate('paid')}
						>
							Simulate paid
						</Button>
						<Button
							size="sm"
							variant="outline"
							disabled={busy !== null}
							onclick={() => simulate('expired')}
						>
							Simulate expired
						</Button>
						<Button
							size="sm"
							variant="destructive"
							disabled={busy !== null}
							onclick={() => simulate('failed')}
						>
							Simulate failed
						</Button>
					</div>
				</section>
			{/if}
		{/snippet}

		{#snippet metadata(payment: AdminPayment)}
			<section
				class="flex flex-col gap-3 rounded-lg border border-border bg-card p-card"
				data-testid="payment-metadata"
			>
				<h2 class="text-card-title">Metadata</h2>
				<JsonBlock value={payment.metadata} />
			</section>
		{/snippet}

		{#if phone.current}
			<div class="flex items-center gap-3">
				<a
					href={resolve('/(admin)/payments')}
					aria-label="Back to payments"
					class="inline-flex size-11 shrink-0 items-center justify-center rounded-full bg-secondary text-foreground"
				>
					<ArrowLeft class="size-5" />
				</a>
				<h2
					class="min-w-0 flex-1 font-mono text-card-title break-all"
					data-testid="payment-order-id"
				>
					{p.order_id}
				</h2>
				<StatusBadge status={p.status} />
			</div>

			<div class="flex flex-col gap-section">
				<PaymentSummary
					part="amount"
					payment={p}
					providerLabel={providerName[p.provider] ?? p.provider}
					methodLabel={p.method ? (methodName[p.method] ?? p.method) : null}
				/>
				<CashInLedger payment={p} cashIn={d.cash_in} />
				<PaymentSummary
					part="details"
					payment={p}
					providerLabel={providerName[p.provider] ?? p.provider}
					methodLabel={p.method ? (methodName[p.method] ?? p.method) : null}
				/>
				<ActivityTimeline
					payment={p}
					events={d.events}
					deliveries={d.deliveries}
					cashIn={d.cash_in}
					{busy}
					onretry={retry}
				/>
				{@render instructions(p)}
				{@render simulator(p)}
				{@render metadata(p)}
			</div>

			<div
				class="sticky bottom-[calc(4rem+env(safe-area-inset-bottom))] z-10 -mx-page flex items-center gap-2 border-t border-border bg-card px-page py-3"
				data-testid="payment-action-bar"
			>
				<Button
					variant="outline"
					size="icon"
					class="size-11 shrink-0"
					aria-label="Sync with provider"
					title="Sync with provider"
					disabled={busy !== null}
					data-testid="sync-payment"
					onclick={() =>
						run('sync', () => adminApi.syncPayment(p.id), 'Payment synced with provider')}
				>
					<RefreshCw class={busy === 'sync' ? 'motion-safe:animate-spin' : undefined} />
				</Button>
				{#if p.snap_url}
					<Button class="min-h-touch flex-1" onclick={() => openExternal(p.snap_url)}>
						<ExternalLink data-icon="inline-start" />
						Open Snap page
					</Button>
				{/if}
			</div>
		{:else}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<div class="flex flex-wrap items-center gap-3">
					<h2 class="font-mono text-page-title break-all" data-testid="payment-order-id">
						{p.order_id}
					</h2>
					<StatusBadge status={p.status} />
				</div>
				<div class="flex flex-wrap gap-2">
					<Button
						variant="outline"
						disabled={busy !== null}
						data-testid="sync-payment"
						onclick={() =>
							run('sync', () => adminApi.syncPayment(p.id), 'Payment synced with provider')}
					>
						<RefreshCw data-icon="inline-start" />
						{busy === 'sync' ? 'Syncing…' : 'Sync with provider'}
					</Button>
					{#if p.snap_url}
						<Button onclick={() => openExternal(p.snap_url)}>
							<ExternalLink data-icon="inline-start" />
							Open Snap page
						</Button>
					{/if}
				</div>
			</div>

			<!-- Below xl one column: summary, ledger, activity, then the rest. From xl two columns. -->
			<div class="grid gap-section xl:grid-cols-[minmax(0,1fr)_380px]">
				<div class="flex min-w-0 flex-col gap-section max-xl:contents">
					<div class="max-xl:order-1">
						<PaymentSummary
							payment={p}
							providerLabel={providerName[p.provider] ?? p.provider}
							methodLabel={p.method ? (methodName[p.method] ?? p.method) : null}
						/>
					</div>
					<div class="max-xl:order-3">
						<ActivityTimeline
							payment={p}
							events={d.events}
							deliveries={d.deliveries}
							cashIn={d.cash_in}
							{busy}
							onretry={retry}
						/>
					</div>
				</div>

				<div class="flex min-w-0 flex-col gap-section max-xl:contents">
					<div class="max-xl:order-2">
						<CashInLedger payment={p} cashIn={d.cash_in} />
					</div>
					<div class="flex min-w-0 flex-col gap-section max-xl:order-4">
						{@render instructions(p)}
						{@render simulator(p)}
						{@render metadata(p)}
					</div>
				</div>
			</div>
		{/if}
	{/if}
</div>
