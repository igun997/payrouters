<script lang="ts">
	import { toast } from 'svelte-sonner';
	import { adminApi, type CashInsQuery } from '#lib/api/admin';
	import { errorMessage } from '#lib/api/client';
	import type { CashInPage, Meta, Product } from '#lib/api/types';
	import { Resource } from '#lib/state/resource.svelte';
	import { prefs } from '#lib/state/prefs.svelte';
	import { shell } from '#lib/state/shell.svelte';
	import { loadMeta, methodLabels, providerLabels } from '#lib/state/catalog.svelte';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';
	import { Button } from '#lib/components/ui/button';
	import PageHeader from '#lib/components/app/page-header.svelte';
	import DateRangeFilter from '#lib/components/app/date-range-filter.svelte';
	import SelectField from '#lib/components/app/select-field.svelte';
	import ResponsiveDialog from '#lib/components/app/responsive-dialog.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import TotalsStrip from '#lib/components/app/totals-strip.svelte';
	import CashInList from '#lib/components/app/cash-in-list.svelte';
	import CursorPager from '#lib/components/app/cursor-pager.svelte';
	import EmptyState from '#lib/components/app/empty-state.svelte';
	import Download from '@lucide/svelte/icons/download';
	import Banknote from '@lucide/svelte/icons/banknote';
	import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
	import X from '@lucide/svelte/icons/x';
	import { formatIdr, formatInt, PROVIDER_LABEL } from '#lib/format';

	const PAGE_SIZE = 25;

	let provider = $state('');
	let method = $state('');
	let productId = $state('');
	let exporting = $state(false);
	let filtersOpen = $state(false);
	const phone = new IsMobile(768);

	const meta = new Resource<Meta>();
	const products = new Resource<{ data: Product[] }>();
	const list = new Resource<CashInPage>();

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
	const methodOptions = $derived([
		{ value: '', label: 'All methods' },
		...(meta.data?.methods ?? []).map((m) => ({ value: m.code, label: m.label }))
	]);
	const productOptions = $derived([
		{ value: '', label: 'All products' },
		...(products.data?.data ?? []).map((p) => ({ value: p.id, label: p.name }))
	]);
	const methodName = $derived(methodLabels(meta.data));
	const providerName = $derived({ ...PROVIDER_LABEL, ...providerLabels(meta.data) });

	/** Active filters as removable chips on phones (one per filter that is set). */
	const activeChips = $derived.by(() => {
		const chips: { key: string; label: string; clear: () => void }[] = [];
		if (productId) {
			const label = productOptions.find((o) => o.value === productId)?.label ?? productId;
			chips.push({ key: 'product', label, clear: () => (productId = '') });
		}
		if (provider) {
			const label = providerOptions.find((o) => o.value === provider)?.label ?? provider;
			chips.push({ key: 'provider', label, clear: () => (provider = '') });
		}
		if (method) {
			const label = methodOptions.find((o) => o.value === method)?.label ?? method;
			chips.push({ key: 'method', label, clear: () => (method = '') });
		}
		return chips;
	});

	// Filters follow the mode switch and the shared range. Cursors reset when any filter changes.
	const filterKey = $derived(
		JSON.stringify([prefs.mode, shell.range.from, shell.range.to, provider, method, productId])
	);
	let trail = $state<{ key: string; cursors: string[] }>({ key: '', cursors: [] });
	const cursors = $derived(trail.key === filterKey ? trail.cursors : []);
	const cursor = $derived(cursors.at(-1));

	const filters = $derived<CashInsQuery>({
		mode: prefs.mode,
		from: shell.range.from,
		to: shell.range.to,
		provider: provider || undefined,
		method: method || undefined,
		product_id: productId || undefined
	});

	$effect(() => {
		const q: CashInsQuery = { ...filters, limit: PAGE_SIZE, cursor };
		void list.load(() => adminApi.listCashIns(q));
	});

	function nextPage() {
		const next = list.data?.next_cursor;
		if (next) trail = { key: filterKey, cursors: [...cursors, next] };
	}

	function prevPage() {
		trail = { key: filterKey, cursors: cursors.slice(0, -1) };
	}

	async function exportCsv() {
		exporting = true;
		try {
			await adminApi.downloadCashInsCsv(
				filters,
				`payrouters-cash-ins-${prefs.mode}-${shell.range.from}-to-${shell.range.to}.csv`
			);
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			exporting = false;
		}
	}

	const rows = $derived(list.data?.data ?? []);
	const totals = $derived(list.data?.totals);
	const pageStart = $derived(cursors.length * PAGE_SIZE + 1);
	const pageEnd = $derived(pageStart + rows.length - 1);
	const totalItems = $derived([
		{ label: 'Payments', value: totals ? formatInt(totals.count) : '—' },
		{ label: 'Gross', value: totals ? formatIdr(totals.gross) : '—' },
		{ label: 'Fees', value: totals ? formatIdr(totals.fee) : '—' },
		{ label: 'Net received', value: totals ? formatIdr(totals.net) : '—', emphasis: true }
	]);
	const headerDescription = $derived(
		`Settled payments in ${prefs.mode} mode, Asia/Jakarta days. Totals cover the whole filter.`
	);
</script>

<PageHeader title="Cash-in recap" description={headerDescription} rangeControl />

<div class="flex flex-col gap-section" data-testid="page-cash-ins">
	<div class="md:hidden">
		<DateRangeFilter variant="segmented" />
	</div>

	{#if phone.current}
		<div class="flex flex-wrap items-center gap-2" data-testid="cash-in-filters">
			<Button
				variant="outline"
				class="min-h-touch"
				onclick={() => (filtersOpen = true)}
				data-testid="open-cash-in-filters"
			>
				<SlidersHorizontal data-icon="inline-start" />
				Filters{activeChips.length > 0 ? ` (${activeChips.length})` : ''}
			</Button>
			{#each activeChips as chip (chip.key)}
				<button
					type="button"
					class="inline-flex min-h-touch items-center gap-1.5 rounded-full border border-input bg-card px-3.5 text-label text-foreground hover:bg-muted"
					data-testid="filter-chip"
					data-filter={chip.key}
					aria-label={`Remove ${chip.label} filter`}
					onclick={chip.clear}
				>
					{chip.label}
					<X class="size-4 text-muted-foreground" />
				</button>
			{/each}
		</div>

		<ResponsiveDialog
			bind:open={filtersOpen}
			title="Filter cash-ins"
			description="Only settled payments that match every filter are listed."
			testid="sheet-cash-in-filters"
		>
			<div class="flex flex-col gap-4">
				<div class="flex flex-col gap-1.5">
					<span class="text-label">Product</span>
					<SelectField
						bind:value={productId}
						options={productOptions}
						label="Product"
						class="w-full"
					/>
				</div>
				<div class="flex flex-col gap-1.5">
					<span class="text-label">Provider</span>
					<SelectField
						bind:value={provider}
						options={providerOptions}
						label="Provider"
						class="w-full"
					/>
				</div>
				<div class="flex flex-col gap-1.5">
					<span class="text-label">Method</span>
					<SelectField bind:value={method} options={methodOptions} label="Method" class="w-full" />
				</div>
			</div>
			{#snippet footer()}
				<Button class="w-full" onclick={() => (filtersOpen = false)}>Apply filters</Button>
			{/snippet}
		</ResponsiveDialog>
	{:else}
		<div class="flex flex-wrap items-center gap-2">
			<SelectField bind:value={provider} options={providerOptions} label="Provider" class="w-44" />
			<SelectField bind:value={method} options={methodOptions} label="Method" class="w-44" />
			<SelectField bind:value={productId} options={productOptions} label="Product" class="w-44" />
			<Button
				variant="outline"
				class="ml-auto"
				onclick={exportCsv}
				disabled={exporting}
				data-testid="export-csv"
			>
				<Download data-icon="inline-start" />
				{exporting ? 'Exporting…' : 'Export CSV'}
			</Button>
		</div>
	{/if}

	<TotalsStrip items={totalItems} testid="cash-in-totals" />

	<ErrorAlert error={list.error} subject="cash-ins" />

	{#if phone.current}
		<div class="flex items-center justify-between gap-3">
			<div class="flex min-w-0 flex-col">
				<h2 class="text-card-title">Settled payments</h2>
				<p class="text-helper text-muted-foreground">
					Page {cursors.length + 1} · {rows.length} shown · Asia/Jakarta days
				</p>
			</div>
			<Button
				variant="outline"
				class="min-h-touch shrink-0"
				onclick={exportCsv}
				disabled={exporting}
				data-testid="export-csv"
			>
				<Download data-icon="inline-start" />
				{exporting ? 'Exporting…' : 'Export CSV'}
			</Button>
		</div>
	{/if}

	<CashInList
		{rows}
		loading={list.loading && !list.data}
		providerLabels={providerName}
		methodLabels={methodName}
	>
		{#snippet empty()}
			<EmptyState
				icon={Banknote}
				title="No cash-ins in this range"
				description="Settled payments appear here once they are paid."
				dashed
			/>
		{/snippet}
	</CashInList>

	{#if rows.length > 0 || cursors.length > 0}
		<CursorPager
			start={pageStart}
			end={pageEnd}
			total={totals?.count ?? null}
			hasPrevious={cursors.length > 0}
			hasNext={Boolean(list.data?.next_cursor)}
			busy={list.loading}
			onprevious={prevPage}
			onnext={nextPage}
		/>
	{/if}
</div>
