<script lang="ts">
	import { adminApi } from '#lib/api/admin';
	import { errorMessage } from '#lib/api/client';
	import type {
		Delivery,
		DeliveryStatus,
		Meta,
		Product,
		ProviderAccount,
		WebhookEvent,
		WebhookOutcome
	} from '#lib/api/types';
	import { Resource } from '#lib/state/resource.svelte';
	import { PagedList } from '#lib/state/paged.svelte';
	import { loadMeta, providerLabels } from '#lib/state/catalog.svelte';
	import { notify } from '#lib/toast';
	import * as Tabs from '#lib/components/ui/tabs';
	import { Button } from '#lib/components/ui/button';
	import PageHeader from '#lib/components/app/page-header.svelte';
	import SelectField from '#lib/components/app/select-field.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import EmptyState from '#lib/components/app/empty-state.svelte';
	import SkeletonBlock from '#lib/components/app/skeleton-block.svelte';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import Inbox from '@lucide/svelte/icons/inbox';
	import Send from '@lucide/svelte/icons/send';
	import WebhookEventRow, { EVENT_COLUMNS } from './webhook-event-row.svelte';
	import DeliveryRow, { DELIVERY_COLUMNS } from './delivery-row.svelte';

	const PAGE_SIZE = 50;

	const meta = new Resource<Meta>();
	const accounts = new Resource<{ data: ProviderAccount[] }>();
	const products = new Resource<{ data: Product[] }>();
	const inbox = new PagedList<WebhookEvent>();
	const outbox = new PagedList<Delivery>();

	let tab = $state<'inbound' | 'outbound'>('inbound');

	// Inbound filters
	let accountId = $state('');
	let outcome = $state<WebhookOutcome | ''>('');
	// Outbound filters
	let productId = $state('');
	let deliveryStatus = $state<DeliveryStatus | ''>('');

	let expandedEvents = $state<Record<string, boolean>>({});
	let expandedDeliveries = $state<Record<string, boolean>>({});
	let retrying = $state<string | null>(null);

	const providerName = $derived(providerLabels(meta.data));
	const inboxFilter = $derived({
		provider_account_id: accountId || undefined,
		outcome: outcome || undefined
	});
	const outboxFilter = $derived({
		product_id: productId || undefined,
		status: deliveryStatus || undefined
	});
	const inboxFiltered = $derived(accountId !== '' || outcome !== '');
	const outboxFiltered = $derived(productId !== '' || deliveryStatus !== '');

	const accountOptions = $derived([
		{ value: '', label: 'All accounts' },
		...(accounts.data?.data ?? []).map((a) => ({ value: a.id, label: `${a.label} (${a.mode})` }))
	]);
	const outcomeOptions: { value: string; label: string }[] = [
		{ value: '', label: 'All outcomes' },
		{ value: 'applied', label: 'Applied' },
		{ value: 'noop', label: 'No-op' },
		{ value: 'unmatched', label: 'Unmatched' },
		{ value: 'rejected', label: 'Rejected' },
		{ value: 'error', label: 'Error' }
	];
	const productOptions = $derived([
		{ value: '', label: 'All products' },
		...(products.data?.data ?? []).map((p) => ({ value: p.id, label: p.name }))
	]);
	const statusOptions: { value: string; label: string }[] = [
		{ value: '', label: 'All statuses' },
		{ value: 'pending', label: 'Pending' },
		{ value: 'delivered', label: 'Delivered' },
		{ value: 'failed', label: 'Failed' }
	];

	const accountName = (id: string): string => {
		const account = accounts.data?.data.find((a) => a.id === id);
		return account ? account.label : id.slice(0, 8);
	};
	const productNameOf = (id: string): string =>
		products.data?.data.find((p) => p.id === id)?.name ?? id.slice(0, 8);
	const targetUrlOf = (id: string): string | null =>
		products.data?.data.find((p) => p.id === id)?.webhook_url ?? null;

	function loadInbox() {
		const filter = inboxFilter;
		void inbox.reset(() => adminApi.listWebhookEvents({ ...filter, limit: PAGE_SIZE }));
	}

	function moreInbox() {
		const filter = inboxFilter;
		void inbox.more((cursor) =>
			adminApi.listWebhookEvents({ ...filter, limit: PAGE_SIZE, cursor })
		);
	}

	function loadOutbox() {
		const filter = outboxFilter;
		void outbox.reset(() => adminApi.listDeliveries({ ...filter, limit: PAGE_SIZE }));
	}

	function moreOutbox() {
		const filter = outboxFilter;
		void outbox.more((cursor) => adminApi.listDeliveries({ ...filter, limit: PAGE_SIZE, cursor }));
	}

	async function retry(delivery: Delivery) {
		retrying = delivery.id;
		try {
			const updated = await adminApi.retryDelivery(delivery.id);
			outbox.update((items) => items.map((d) => (d.id === updated.id ? updated : d)));
			notify.success('Delivery queued for retry');
		} catch (error) {
			notify.errorWithAction(errorMessage(error), {
				label: 'Retry',
				onClick: () => void retry(delivery)
			});
		} finally {
			retrying = null;
		}
	}

	$effect(() => {
		void meta.load(loadMeta);
		void accounts.load(() => adminApi.listProviders());
		void products.load(() => adminApi.listProducts());
	});

	// Each filter change starts a fresh first page; stale responses are ignored by PagedList.
	$effect(() => {
		loadInbox();
	});
	$effect(() => {
		loadOutbox();
	});
</script>

<div data-testid="page-webhooks" class="flex flex-col gap-section">
	<PageHeader
		title="Webhooks"
		description="Inbound provider events and outbound deliveries to your products."
	/>

	<Tabs.Root bind:value={tab} class="flex flex-col gap-section">
		<Tabs.List
			class="grid h-auto min-h-touch w-full grid-cols-2 gap-1 rounded-lg bg-secondary p-1"
			data-testid="webhook-tabs"
		>
			<Tabs.Trigger
				value="inbound"
				class="h-auto min-h-touch flex-none rounded-md px-3 text-label font-semibold text-muted-foreground data-[state=active]:bg-card data-[state=active]:text-foreground data-[state=active]:shadow-sm"
			>
				Inbound events
			</Tabs.Trigger>
			<Tabs.Trigger
				value="outbound"
				class="h-auto min-h-touch flex-none rounded-md px-3 text-label font-semibold text-muted-foreground data-[state=active]:bg-card data-[state=active]:text-foreground data-[state=active]:shadow-sm"
			>
				Outbound deliveries
			</Tabs.Trigger>
		</Tabs.List>

		<Tabs.Content value="inbound" class="flex flex-col gap-section" data-testid="inbound-panel">
			<div class="flex flex-wrap items-center gap-2">
				<div data-testid="filter-account" class="w-full md:w-72">
					<SelectField
						bind:value={accountId}
						options={accountOptions}
						label="Provider account"
						class="w-full"
					/>
				</div>
				<div data-testid="filter-outcome" class="w-full md:w-56">
					<SelectField
						bind:value={outcome}
						options={outcomeOptions}
						label="Outcome"
						class="w-full"
					/>
				</div>
				<span class="hidden text-helper text-muted-foreground md:ml-auto md:inline">
					Newest first · times in WIB
				</span>
			</div>

			<ErrorAlert error={inbox.error} onretry={loadInbox} />

			<div class="overflow-hidden rounded-lg border bg-card">
				<div
					class="hidden gap-x-4 border-b px-6 py-3 text-helper font-semibold text-muted-foreground md:grid {EVENT_COLUMNS}"
				>
					<span aria-hidden="true"></span>
					<span>Received</span>
					<span>Provider / account</span>
					<span>Outcome</span>
					<span>Payment and error</span>
				</div>

				{#if inbox.loading && inbox.items.length === 0}
					<ul class="divide-y" aria-busy="true" aria-label="Loading events">
						{#each Array.from({ length: 5 }, (_, i) => i) as i (i)}
							<li class="flex flex-col gap-3 p-card md:flex-row md:items-center md:px-6 md:py-4">
								<SkeletonBlock class="h-4 w-40" />
								<SkeletonBlock class="h-4 w-full md:flex-1" />
							</li>
						{/each}
					</ul>
				{:else if inbox.items.length === 0}
					<EmptyState
						icon={Inbox}
						title={inboxFiltered ? 'No events match these filters' : 'No inbound events'}
						description={inboxFiltered
							? undefined
							: 'Provider webhooks land here, verified or not.'}
						class="py-12"
					>
						{#snippet actions()}
							{#if inboxFiltered}
								<Button
									variant="outline"
									onclick={() => {
										accountId = '';
										outcome = '';
									}}
								>
									Clear filters
								</Button>
							{/if}
						{/snippet}
					</EmptyState>
				{:else}
					<ul class="divide-y" data-testid="webhook-event-list">
						{#each inbox.items as event (event.id)}
							<WebhookEventRow
								{event}
								providerName={providerName[event.provider] ?? event.provider}
								accountName={accountName(event.provider_account_id)}
								expanded={!!expandedEvents[event.id]}
								ontoggle={() =>
									(expandedEvents = { ...expandedEvents, [event.id]: !expandedEvents[event.id] })}
							/>
						{/each}
					</ul>
				{/if}
			</div>

			{#if inbox.nextCursor}
				<div class="flex justify-center">
					<Button
						variant="outline"
						onclick={moreInbox}
						disabled={inbox.loadingMore}
						data-testid="load-more-events"
					>
						<ChevronDown data-icon="inline-start" />
						Load more
					</Button>
				</div>
			{/if}
		</Tabs.Content>

		<Tabs.Content value="outbound" class="flex flex-col gap-section" data-testid="outbound-panel">
			<div class="flex flex-wrap items-center gap-2">
				<div data-testid="filter-product" class="w-full md:w-60">
					<SelectField
						bind:value={productId}
						options={productOptions}
						label="Product"
						class="w-full"
					/>
				</div>
				<div data-testid="filter-status" class="w-full md:w-52">
					<SelectField
						bind:value={deliveryStatus}
						options={statusOptions}
						label="Status"
						class="w-full"
					/>
				</div>
				<span class="hidden text-helper text-muted-foreground md:ml-auto md:inline">
					Retries: 10 s, 1 m, 5 m, 30 m, 2 h, 6 h, 12 h, 24 h
				</span>
			</div>

			<ErrorAlert error={outbox.error} onretry={loadOutbox} />

			<div class="overflow-hidden rounded-lg border bg-card">
				<div
					class="hidden gap-x-4 border-b px-6 py-3 text-helper font-semibold text-muted-foreground md:grid {DELIVERY_COLUMNS}"
				>
					<span>Event</span>
					<span>Product</span>
					<span>Status</span>
					<span>Last response</span>
					<span class="sr-only">Actions</span>
				</div>

				{#if outbox.loading && outbox.items.length === 0}
					<ul class="divide-y" aria-busy="true" aria-label="Loading deliveries">
						{#each Array.from({ length: 5 }, (_, i) => i) as i (i)}
							<li class="flex flex-col gap-3 p-card md:flex-row md:items-center md:px-6 md:py-4">
								<SkeletonBlock class="h-4 w-44" />
								<SkeletonBlock class="h-4 w-full md:flex-1" />
							</li>
						{/each}
					</ul>
				{:else if outbox.items.length === 0}
					<EmptyState
						icon={Send}
						title={outboxFiltered ? 'No deliveries match these filters' : 'No deliveries'}
						description={outboxFiltered
							? undefined
							: 'Deliveries appear when a payment changes status.'}
						class="py-12"
					>
						{#snippet actions()}
							{#if outboxFiltered}
								<Button
									variant="outline"
									onclick={() => {
										productId = '';
										deliveryStatus = '';
									}}
								>
									Clear filters
								</Button>
							{/if}
						{/snippet}
					</EmptyState>
				{:else}
					<ul class="divide-y" data-testid="delivery-list">
						{#each outbox.items as delivery (delivery.id)}
							<DeliveryRow
								{delivery}
								productName={productNameOf(delivery.product_id)}
								targetUrl={targetUrlOf(delivery.product_id)}
								expanded={!!expandedDeliveries[delivery.id]}
								retrying={retrying === delivery.id}
								ontoggle={() =>
									(expandedDeliveries = {
										...expandedDeliveries,
										[delivery.id]: !expandedDeliveries[delivery.id]
									})}
								onretry={() => void retry(delivery)}
							/>
						{/each}
					</ul>
				{/if}
			</div>

			{#if outbox.nextCursor}
				<div class="flex justify-center">
					<Button
						variant="outline"
						onclick={moreOutbox}
						disabled={outbox.loadingMore}
						data-testid="load-more-deliveries"
					>
						<ChevronDown data-icon="inline-start" />
						Load more
					</Button>
				</div>
			{/if}
		</Tabs.Content>
	</Tabs.Root>
</div>
