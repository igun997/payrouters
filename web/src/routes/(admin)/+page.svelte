<script lang="ts">
	import { resolve } from '$app/paths';
	import { adminApi, type PaymentsQuery, type StatsQuery } from '#lib/api/admin';
	import type {
		AdminPayment,
		BreakdownRow,
		Meta,
		Page,
		StatsSummary,
		TimeseriesPoint
	} from '#lib/api/types';
	import { Resource } from '#lib/state/resource.svelte';
	import { prefs } from '#lib/state/prefs.svelte';
	import { shell } from '#lib/state/shell.svelte';
	import { loadMeta, methodLabels, providerLabels } from '#lib/state/catalog.svelte';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import PageHeader from '#lib/components/app/page-header.svelte';
	import StatGrid from '#lib/components/app/stat-grid.svelte';
	import KpiCard from '#lib/components/app/kpi-card.svelte';
	import BarChart from '#lib/components/app/bar-chart.svelte';
	import BarList from '#lib/components/app/bar-list.svelte';
	import PaymentList from '#lib/components/app/payment-list.svelte';
	import DateRangeFilter from '#lib/components/app/date-range-filter.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import {
		fullDay,
		formatRatio,
		inclusiveDays,
		rangeLabel,
		shareOf,
		shortDay
	} from '#lib/admin-view';
	import { formatIdr, formatInt, PROVIDER_LABEL, percentChange } from '#lib/format';

	const RECENT = 5;

	const meta = new Resource<Meta>();
	const summary = new Resource<StatsSummary>();
	const series = new Resource<TimeseriesPoint[]>();
	const byProvider = new Resource<BreakdownRow[]>();
	const recent = new Resource<Page<AdminPayment>>();

	$effect(() => {
		void meta.load(loadMeta);
	});

	const query = $derived<StatsQuery>({
		mode: prefs.mode,
		from: shell.range.from,
		to: shell.range.to
	});

	function load(q: StatsQuery) {
		const recentQuery: PaymentsQuery = { ...q, limit: RECENT };
		void summary.load(() => adminApi.statsSummary(q));
		void series.load(() => adminApi.statsTimeseries(q).then((r) => r.data));
		void byProvider.load(() => adminApi.statsBreakdown('provider', q).then((r) => r.data));
		void recent.load(() => adminApi.listPayments(recentQuery));
	}

	$effect(() => {
		load(query);
	});

	function reload() {
		load(query);
	}

	const providerName = $derived({ ...PROVIDER_LABEL, ...providerLabels(meta.data) });
	const methodName = $derived(methodLabels(meta.data));

	const s = $derived(summary.data);
	const days = $derived(inclusiveDays(shell.range.from, shell.range.to));
	const changeLabel = $derived(`vs previous ${days} days`);
	const closed = $derived(
		s ? s.payments.paid + s.payments.expired + s.payments.failed + s.payments.cancelled : 0
	);
	const noData = $derived(s !== undefined && s.payments.total === 0);
	const rangeText = $derived(rangeLabel(shell.range.from, shell.range.to));
	const emptyHint = $derived(noData ? 'No data in this range' : undefined);

	/** Relative change vs the previous equal-length range; null when not comparable. */
	function change(now: number, before: number | undefined): number | null {
		if (before === undefined || (now === 0 && before === 0)) return null;
		return percentChange(now, before);
	}

	const chartData = $derived(
		(series.data ?? []).map((p) => ({
			label: shortDay(p.date),
			value: p.gross,
			title: `${fullDay(p.date)}: ${formatInt(p.count)} paid, ${formatIdr(p.gross)} gross, ${formatIdr(p.net)} net`
		}))
	);

	const peak = $derived(
		(series.data ?? []).reduce<TimeseriesPoint | null>(
			(best, p) => (p.gross > 0 && (!best || p.gross > best.gross) ? p : best),
			null
		)
	);

	const providerRows = $derived.by(() => {
		const rows = byProvider.data ?? [];
		const total = rows.reduce((sum, r) => sum + r.gross, 0);
		return rows.map((r) => {
			const share = shareOf(r.gross, total);
			return {
				key: r.key,
				label: providerName[r.key] ?? r.label,
				provider: r.key,
				amount: r.gross,
				share,
				detail: `${formatRatio(share)} · ${formatInt(r.paid)} payments`
			};
		});
	});

	const headerDescription = $derived(
		`Cash-in performance in ${prefs.mode === 'live' ? 'live' : 'sandbox'} mode.`
	);
</script>

<PageHeader title="Overview" description={headerDescription} rangeControl />

<div class="md:hidden">
	<DateRangeFilter variant="segmented" />
</div>

<ErrorAlert
	subject="the overview"
	error={summary.error ?? series.error ?? byProvider.error ?? recent.error}
	onretry={reload}
/>

{#if s}
	<div class="md:hidden">
		<KpiCard
			hero
			title={`Net received, ${rangeText}`}
			value={formatIdr(s.cash_in.net)}
			change={change(s.cash_in.net, s.previous.net)}
			{changeLabel}
			details={[
				{ label: 'Gross', value: formatIdr(s.cash_in.gross) },
				{ label: 'Fees', value: formatIdr(s.cash_in.fee) }
			]}
			testid="kpi-net-hero"
		/>
	</div>

	<StatGrid testid="stat-grid">
		<div class="grid md:hidden">
			<KpiCard
				title="Paid payments"
				value={formatInt(s.payments.paid)}
				change={change(s.cash_in.count, s.previous.count)}
				{changeLabel}
				hint={emptyHint}
				testid="kpi-paid"
			/>
		</div>
		<div class="hidden md:grid">
			<KpiCard
				title="Gross cash-in"
				value={formatIdr(s.cash_in.gross)}
				change={change(s.cash_in.gross, s.previous.gross)}
				{changeLabel}
				hint={emptyHint}
				testid="kpi-gross"
			/>
		</div>
		<div class="hidden md:grid">
			<KpiCard
				title="Net received"
				value={formatIdr(s.cash_in.net)}
				change={change(s.cash_in.net, s.previous.net)}
				{changeLabel}
				hint={emptyHint}
				testid="kpi-net"
			/>
		</div>
		<div class="hidden md:grid">
			<KpiCard
				title="Fees"
				value={formatIdr(s.cash_in.fee)}
				hint={emptyHint ?? `${formatRatio(shareOf(s.cash_in.fee, s.cash_in.gross))} of gross`}
				testid="kpi-fees"
			/>
		</div>
		<KpiCard
			title="Conversion rate"
			value={closed > 0 ? formatRatio(s.conversion_rate) : '—'}
			hint={emptyHint ?? `${formatInt(s.payments.paid)} paid of ${formatInt(closed)} closed`}
			testid="kpi-conversion"
		/>
	</StatGrid>
{:else if summary.loading}
	<div class="grid grid-cols-2 gap-section xl:grid-cols-4" data-testid="overview-skeleton-stats">
		{#each Array.from({ length: 4 }, (_, i) => i) as i (i)}
			<div class="flex flex-col gap-2 rounded-lg border border-border bg-card p-card">
				<Skeleton class="h-4 w-24" />
				<Skeleton class="h-7 w-32" />
				<Skeleton class="h-3.5 w-40" />
			</div>
		{/each}
	</div>
{/if}

<div class="grid gap-section xl:grid-cols-[minmax(0,1fr)_380px]">
	<section
		class="flex min-w-0 flex-col gap-4 rounded-lg border border-border bg-card p-card"
		data-testid="chart-card"
	>
		<div class="flex flex-col gap-1">
			<h2 class="text-card-title">Gross cash-in per day</h2>
			<p class="text-helper text-muted-foreground">
				{#if peak}
					Peak {formatIdr(peak.gross)} on {shortDay(peak.date)}.<span class="hidden md:inline">
						&#32;Days follow Asia/Jakarta.
					</span>
				{:else}
					Paid amounts by day, Asia/Jakarta.
				{/if}
			</p>
		</div>
		{#if series.loading && !series.data}
			<Skeleton class="h-56 w-full" />
		{:else}
			<BarChart
				data={chartData}
				empty={`No cash-in between ${shortDay(shell.range.from)} and ${shortDay(shell.range.to)}`}
			/>
		{/if}
	</section>

	<section
		class="flex min-w-0 flex-col gap-4 rounded-lg border border-border bg-card p-card"
		data-testid="provider-card"
	>
		<div class="flex flex-col gap-1">
			<h2 class="text-card-title">By provider</h2>
			<p class="text-helper text-muted-foreground">Share of gross cash-in</p>
		</div>
		{#if byProvider.loading && !byProvider.data}
			<div class="flex flex-col gap-3">
				<Skeleton class="h-12 w-full" />
				<Skeleton class="h-12 w-full" />
			</div>
		{:else}
			<BarList rows={providerRows} format={formatIdr} empty="No provider activity in this range." />
		{/if}
	</section>
</div>

<section
	class="flex flex-col overflow-hidden rounded-lg border border-border bg-card"
	data-testid="recent-section"
>
	<div class="flex items-center justify-between gap-3 px-card">
		<h2 class="text-card-title">Recent payments</h2>
		<a
			href={resolve('/(admin)/payments')}
			class="inline-flex min-h-touch items-center text-label underline-offset-4 hover:underline"
		>
			View all
			<ChevronRight class="size-4" />
		</a>
	</div>
	<PaymentList
		payments={recent.data?.data ?? []}
		loading={(recent.loading && !recent.data) || (meta.loading && !meta.data)}
		variant="recent"
		bare
		providerLabels={providerName}
		methodLabels={methodName}
		testid="recent-payments-list"
	>
		{#snippet empty()}
			<div class="flex flex-col items-center gap-1 px-card py-10 text-center">
				<p class="text-card-title">No payments in this range</p>
				<p class="text-body text-muted-foreground">Payments in {rangeText} appear here.</p>
			</div>
		{/snippet}
	</PaymentList>
</section>
