<script lang="ts">
	import { resolve } from '$app/paths';
	import { adminApi } from '#lib/api/admin';
	import type { AdminPayment, Meta, Page, Product, ProviderAccount } from '#lib/api/types';
	import { Resource } from '#lib/state/resource.svelte';
	import { prefs } from '#lib/state/prefs.svelte';
	import { loadMeta, methodLabels, providerLabels } from '#lib/state/catalog.svelte';
	import { notify } from '#lib/toast';
	import PageHeader from '#lib/components/app/page-header.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import StatusBadge from '#lib/components/app/status-badge.svelte';
	import SkeletonBlock from '#lib/components/app/skeleton-block.svelte';
	import { formatDateTime, formatIdr } from '#lib/format';
	import PaymentForm from './payment-form.svelte';
	import ResultCard from './result-card.svelte';
	import { buildPaymentBody, type PaymentDraft } from './draft';

	const meta = new Resource<Meta>();
	const accounts = new Resource<{ data: ProviderAccount[] }>();
	const products = new Resource<{ data: Product[] }>();
	const recent = new Resource<Page<AdminPayment>>();

	let created = $state<AdminPayment | null>(null);
	let submitting = $state(false);
	let submitError = $state<unknown>(null);

	const mode = $derived(prefs.mode);
	const methodName = $derived(methodLabels(meta.data));
	const providerName = $derived(providerLabels(meta.data));

	function loadRecent() {
		const current = prefs.mode;
		void recent.load(() => adminApi.listPayments({ mode: current, limit: 10 }));
	}

	$effect(() => {
		void meta.load(loadMeta);
		void accounts.load(() => adminApi.listProviders());
		void products.load(() => adminApi.listProducts());
	});

	// Recent payments follow the mode switch; a result from the other mode is no longer relevant.
	$effect(() => {
		void prefs.mode;
		created = null;
		loadRecent();
	});

	async function create(draft: PaymentDraft) {
		submitting = true;
		submitError = null;
		try {
			const payment = await adminApi.createPayment(buildPaymentBody(draft, mode));
			created = payment;
			notify.success(`Created ${payment.order_id}`);
			loadRecent();
		} catch (error) {
			submitError = error;
		} finally {
			submitting = false;
		}
	}

	function resultMethod(payment: AdminPayment): string {
		if (payment.method) return methodName[payment.method] ?? payment.method;
		return payment.checkout === 'provider' ? 'Provider-hosted page' : 'Chosen on the Snap page';
	}
</script>

<div data-testid="page-playground" class="flex flex-col gap-section">
	<PageHeader
		title={mode === 'sandbox' ? 'Sandbox playground' : 'Live playground'}
		phoneTitle="Playground"
		phoneDescription="Create a payment through the admin API. No product key needed."
		description={mode === 'sandbox'
			? 'Create a payment against your sandbox accounts and open its checkout.'
			: 'Create a payment against your live accounts. These are real payments.'}
	/>

	<div class="grid gap-section xl:grid-cols-[minmax(0,1fr)_400px]">
		<section class="min-w-0 rounded-lg border bg-card p-card" data-testid="playground-form-card">
			{#if (products.loading && !products.data) || (accounts.loading && !accounts.data) || (meta.loading && !meta.data)}
				<div class="flex flex-col gap-5" aria-busy="true" data-testid="playground-form-skeleton">
					{#each Array.from({ length: 5 }, (_, i) => i) as i (i)}
						<div class="flex flex-col gap-2">
							<SkeletonBlock class="h-4 w-24" />
							<SkeletonBlock class="h-11 w-full" />
						</div>
					{/each}
				</div>
			{:else}
				<PaymentForm
					{mode}
					products={products.data?.data ?? []}
					accounts={accounts.data?.data ?? []}
					meta={meta.data}
					{submitting}
					error={submitError}
					onsubmit={create}
				/>
			{/if}
		</section>

		<div class="flex min-w-0 flex-col gap-section">
			{#if created}
				<ResultCard
					payment={created}
					providerName={providerName[created.provider] ?? created.provider}
					methodLabel={resultMethod(created)}
				/>
			{/if}

			<section
				class="flex flex-col gap-3 rounded-lg border bg-card p-card"
				data-testid="recent-payments"
			>
				<div class="flex flex-col gap-1">
					<h2 class="text-card-title">Recent {mode} payments</h2>
					<p class="text-helper text-muted-foreground">The ten newest {mode} payments.</p>
				</div>

				{#if recent.loading && !recent.data}
					<div class="flex flex-col gap-4 py-2" aria-busy="true">
						{#each Array.from({ length: 3 }, (_, i) => i) as i (i)}
							<div class="flex items-center justify-between gap-3">
								<SkeletonBlock class="h-4 w-32" />
								<SkeletonBlock class="h-5 w-20" />
							</div>
						{/each}
					</div>
				{:else if recent.error}
					<ErrorAlert error={recent.error} subject="recent payments" onretry={loadRecent} />
				{:else if (recent.data?.data ?? []).length === 0}
					<p class="py-6 text-center text-body text-muted-foreground" data-testid="recent-empty">
						No {mode} payments yet.
					</p>
				{:else}
					<ul class="divide-y" data-testid="recent-list">
						{#each recent.data?.data ?? [] as payment (payment.id)}
							<li class="flex items-center justify-between gap-3 py-3" data-testid="recent-row">
								<div class="flex min-w-0 flex-col gap-0.5">
									<a
										href={resolve('/(admin)/payments/[id]', { id: payment.id })}
										class="inline-flex min-h-touch w-fit max-w-full items-center text-mono-id font-mono break-all underline-offset-4 hover:underline"
									>
										{payment.order_id}
									</a>
									<span class="text-helper break-words text-muted-foreground">
										{payment.product.name} · {formatDateTime(payment.created_at)}
									</span>
								</div>
								<div class="flex shrink-0 flex-col items-end gap-1.5">
									<span class="tabular text-cell font-semibold whitespace-nowrap">
										{formatIdr(payment.amount)}
									</span>
									<StatusBadge status={payment.status} />
								</div>
							</li>
						{/each}
					</ul>
				{/if}
			</section>
		</div>
	</div>
</div>
