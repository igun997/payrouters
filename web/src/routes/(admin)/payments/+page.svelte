<script lang="ts">
	import { resolve } from '$app/paths';
	import { adminApi, type PaymentsQuery } from '#lib/api/admin';
	import type { AdminPayment, Meta, Page, Product, StatsSummary } from '#lib/api/types';
	import { Resource } from '#lib/state/resource.svelte';
	import { prefs } from '#lib/state/prefs.svelte';
	import { shell } from '#lib/state/shell.svelte';
	import { loadMeta, methodLabels, providerLabels } from '#lib/state/catalog.svelte';
	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import PageHeader from '#lib/components/app/page-header.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import SelectField from '#lib/components/app/select-field.svelte';
	import DateRangeFilter from '#lib/components/app/date-range-filter.svelte';
	import PaymentList from '#lib/components/app/payment-list.svelte';
	import PaymentStatusChips from '#lib/components/app/payment-status-chips.svelte';
	import PaymentFilterSheet from '#lib/components/app/payment-filter-sheet.svelte';
	import DateRangeSheet from '#lib/components/app/date-range-sheet.svelte';
	import CursorPager from '#lib/components/app/cursor-pager.svelte';
	import EmptyState from '#lib/components/app/empty-state.svelte';
	import Search from '@lucide/svelte/icons/search';
	import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
	import CreditCard from '@lucide/svelte/icons/credit-card';
	import SearchX from '@lucide/svelte/icons/search-x';
	import FunnelX from '@lucide/svelte/icons/funnel-x';
	import FlaskConical from '@lucide/svelte/icons/flask-conical';
	import { PROVIDER_LABEL } from '#lib/format';
	import { rangeLabel } from '#lib/admin-view';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';

	const PAGE_SIZE = 25;
	const statusOptions = [
		{ value: '', label: 'All statuses' },
		{ value: 'pending', label: 'Pending' },
		{ value: 'paid', label: 'Paid' },
		{ value: 'expired', label: 'Expired' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'cancelled', label: 'Cancelled' }
	];

	let status = $state('');
	let provider = $state('');
	let productId = $state('');
	let searchInput = $state('');
	let q = $state('');
	let sheetOpen = $state(false);
	let rangeOpen = $state(false);
	const phone = new IsMobile(768);

	// Debounced search: the committed query follows typing after a short pause.
	$effect(() => {
		const value = searchInput.trim();
		const timer = setTimeout(() => (q = value), 350);
		return () => clearTimeout(timer);
	});

	const meta = new Resource<Meta>();
	const products = new Resource<{ data: Product[] }>();
	const summary = new Resource<StatsSummary>();
	const list = new Resource<Page<AdminPayment>>();

	$effect(() => {
		void meta.load(loadMeta);
		void products.load(() => adminApi.listProducts());
	});

	const providerOptions = $derived([
		{ value: '', label: 'All providers' },
		...Object.entries({ ...PROVIDER_LABEL, ...providerLabels(meta.data) }).map(
			([value, label]) => ({
				value,
				label
			})
		)
	]);
	const productOptions = $derived([
		{ value: '', label: 'All products' },
		...(products.data?.data ?? []).map((p) => ({ value: p.id, label: p.name }))
	]);
	const methodName = $derived(methodLabels(meta.data));
	const providerName = $derived({ ...PROVIDER_LABEL, ...providerLabels(meta.data) });

	const hasFilters = $derived(Boolean(status || provider || productId || q));

	// Keyset pagination: the trail of cursors resets whenever a filter or the range changes.
	const filterKey = $derived(
		JSON.stringify([prefs.mode, status, provider, productId, q, shell.range.from, shell.range.to])
	);
	let trail = $state<{ key: string; cursors: string[] }>({ key: '', cursors: [] });
	const cursors = $derived(trail.key === filterKey ? trail.cursors : []);
	const cursor = $derived(cursors.at(-1));

	const listQuery = $derived<PaymentsQuery>({
		mode: prefs.mode,
		status: status || undefined,
		provider: provider || undefined,
		product_id: productId || undefined,
		q: q || undefined,
		from: shell.range.from,
		to: shell.range.to,
		limit: PAGE_SIZE,
		cursor
	});

	$effect(() => {
		const current = listQuery;
		void list.load(() => adminApi.listPayments(current));
	});

	// Chip counts come from the summary for the same mode, range, provider and product.
	$effect(() => {
		const current = {
			mode: prefs.mode,
			from: shell.range.from,
			to: shell.range.to,
			provider: provider || undefined,
			product_id: productId || undefined
		};
		void summary.load(() => adminApi.statsSummary(current));
	});

	const counts = $derived.by(() => {
		const p = summary.data?.payments;
		if (!p) return {};
		return { '': p.total, paid: p.paid, pending: p.pending, expired: p.expired, failed: p.failed };
	});

	/** The total for the pager: exact when the filters are status and range only. */
	const total = $derived.by<number | null>(() => {
		if (q) return null;
		if (!status) return counts[''] ?? null;
		return counts[status as 'paid'] ?? null;
	});

	const rows = $derived(list.data?.data ?? []);
	const pageStart = $derived(cursors.length * PAGE_SIZE + 1);
	const pageEnd = $derived(pageStart + rows.length - 1);

	function nextPage() {
		const next = list.data?.next_cursor;
		if (next) trail = { key: filterKey, cursors: [...cursors, next] };
	}

	function prevPage() {
		trail = { key: filterKey, cursors: cursors.slice(0, -1) };
	}

	function clearFilters() {
		status = '';
		provider = '';
		productId = '';
		searchInput = '';
		q = '';
	}

	function reload() {
		const current = listQuery;
		void list.load(() => adminApi.listPayments(current));
	}
</script>

<PageHeader title="Payments" description="Every payment across products, filtered and paged." />

<div class="flex flex-col gap-section" data-testid="page-payments">
	<div class="flex flex-col-reverse gap-3 min-[360px]:flex-col md:flex-col-reverse">
		<div class="flex flex-col gap-2 xl:flex-row xl:items-center">
			<div class="flex items-center gap-2 xl:min-w-0 xl:flex-1">
				<form class="relative min-w-0 flex-1" role="search" onsubmit={(e) => e.preventDefault()}>
					<Search
						class="pointer-events-none absolute top-1/2 left-3.5 size-4 -translate-y-1/2 text-muted-foreground"
					/>
					<Input
						class="pl-10"
						placeholder={phone.current
							? 'Search order or ref'
							: 'Search order ID, payment ID or provider ref'}
						aria-label="Search payments"
						bind:value={searchInput}
						data-testid="payments-search"
					/>
				</form>
				<Button
					variant="outline"
					size="icon"
					class="md:hidden"
					aria-label="Open filters"
					onclick={() => (sheetOpen = true)}
				>
					<SlidersHorizontal />
				</Button>
			</div>

			<div class="hidden flex-wrap items-center gap-2 md:flex">
				<SelectField
					bind:value={provider}
					options={providerOptions}
					label="Provider"
					class="w-44"
				/>
				<SelectField bind:value={productId} options={productOptions} label="Product" class="w-44" />
				<DateRangeFilter />
				{#if hasFilters}
					<Button variant="ghost" size="sm" onclick={clearFilters}>
						<FunnelX data-icon="inline-start" />
						Clear filters
					</Button>
				{/if}
			</div>
		</div>
		<PaymentStatusChips {counts} value={status} onchange={(next: string) => (status = next)} />
	</div>

	<ErrorAlert error={list.error} subject="payments" onretry={reload} />

	{#if !hasFilters && !list.loading && rows.length === 0 && !list.error}
		<EmptyState
			icon={CreditCard}
			title={`No payments in ${prefs.mode} yet`}
			description={prefs.mode === 'sandbox'
				? 'Create a test payment in the Playground and it shows up here within seconds.'
				: 'Payments from live products appear here once they are created.'}
			dashed
			class="min-h-96"
		>
			{#snippet actions()}
				<Button href={resolve('/(admin)/playground')}>
					<FlaskConical data-icon="inline-start" />
					Open Playground
				</Button>
			{/snippet}
		</EmptyState>
	{:else}
		<PaymentList
			payments={rows}
			loading={list.loading && !list.data}
			variant="full"
			providerLabels={providerName}
			methodLabels={methodName}
			testid="payments-list"
		>
			{#snippet empty()}
				{#if hasFilters}
					<EmptyState
						icon={SearchX}
						title="No payments match these filters"
						description="Try another status, provider or date range."
						dashed
					>
						{#snippet actions()}
							<Button variant="outline" onclick={clearFilters}>
								<FunnelX data-icon="inline-start" />
								Clear filters
							</Button>
						{/snippet}
					</EmptyState>
				{:else}
					<EmptyState
						icon={CreditCard}
						title={`No payments in ${prefs.mode} yet`}
						description={prefs.mode === 'sandbox'
							? 'Create a test payment in the Playground and it shows up here within seconds.'
							: 'Payments from live products appear here once they are created.'}
						dashed
					>
						{#snippet actions()}
							<Button href={resolve('/(admin)/playground')}>
								<FlaskConical data-icon="inline-start" />
								Open Playground
							</Button>
						{/snippet}
					</EmptyState>
				{/if}
			{/snippet}
		</PaymentList>
	{/if}

	{#if rows.length > 0 || cursors.length > 0}
		<CursorPager
			start={pageStart}
			end={pageEnd}
			{total}
			hasPrevious={cursors.length > 0}
			hasNext={Boolean(list.data?.next_cursor)}
			busy={list.loading}
			onprevious={prevPage}
			onnext={nextPage}
		/>
	{/if}
</div>

<PaymentFilterSheet
	bind:open={sheetOpen}
	bind:status
	bind:provider
	bind:productId
	{statusOptions}
	{providerOptions}
	{productOptions}
	rangeLabel={rangeLabel(shell.range.from, shell.range.to)}
	onrange={() => {
		sheetOpen = false;
		rangeOpen = true;
	}}
/>

<DateRangeSheet bind:open={rangeOpen} onclose={() => (sheetOpen = true)} />
