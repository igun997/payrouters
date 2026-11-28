<script lang="ts">
	/*
	 * Customer payment page. The URL token is the capability; no login. State comes from
	 * GET /snap/api/:token: polled every 3 s while pending, paused while the tab is hidden, and
	 * refreshed as soon as the tab is visible again.
	 */
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { toast } from 'svelte-sonner';
	import { snapApi } from '#lib/api/snap';
	import { ApiError, errorMessage } from '#lib/api/client';
	import type { Method, SimulateOutcome, SnapState } from '#lib/api/types';
	import { Button } from '#lib/components/ui/button';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import AmountBlock from '#lib/components/snap/amount-block.svelte';
	import InstructionsView from '#lib/components/snap/instructions-view.svelte';
	import MethodPicker from '#lib/components/snap/method-picker.svelte';
	import OrderMeta from '#lib/components/snap/order-meta.svelte';
	import PayBefore from '#lib/components/snap/pay-before.svelte';
	import PaymentResult from '#lib/components/snap/payment-result.svelte';
	import ResultHero from '#lib/components/snap/result-hero.svelte';
	import Unlink from '@lucide/svelte/icons/unlink';
	import SnapShell from '#lib/components/snap/snap-shell.svelte';
	import { safeHttpUrl } from '#lib/url';

	const POLL_MS = 3000;

	const token = $derived(page.params.token ?? '');

	let snap = $state<SnapState | null>(null);
	let notFound = $state(false);
	let loadError = $state<string | null>(null);
	let visible = $state(true);
	let connecting = $state<Method | null>(null);
	let failed = $state<{ method: Method; message: string } | null>(null);
	let simulating = $state(false);
	// Every request takes a number; a response is applied only if no newer request started after it.
	// That keeps a slow poll from overwriting the result of a pay or simulate call.
	let latest = 0;

	const payment = $derived(snap?.payment ?? null);
	const pending = $derived(payment?.status === 'pending');
	const merchant = $derived(snap?.merchant_name ?? null);
	const total = $derived(payment ? (payment.payable_amount ?? payment.amount) : 0);
	const backUrl = $derived(safeHttpUrl(payment?.return_url ?? null));

	async function load(current: string, silent: boolean) {
		const seq = ++latest;
		try {
			const next = await snapApi.get(current);
			if (seq !== latest) return;
			snap = next;
			notFound = false;
			loadError = null;
		} catch (error) {
			if (seq !== latest) return;
			if (error instanceof ApiError && error.status === 404) {
				notFound = true;
				snap = null;
			} else if (!silent || !snap) {
				loadError = errorMessage(error);
			}
		}
	}

	// New token: reset everything and load the payment.
	$effect(() => {
		const current = token;
		snap = null;
		notFound = false;
		loadError = null;
		connecting = null;
		failed = null;
		void load(current, false);
	});

	// Poll while pending and visible. Terminal statuses stop the timer.
	$effect(() => {
		if (!pending || !visible) return;
		const current = token;
		const timer = setInterval(() => void load(current, true), POLL_MS);
		return () => clearInterval(timer);
	});

	onMount(() => {
		const onVisibilityChange = () => {
			visible = document.visibilityState === 'visible';
			if (visible) void load(token, true);
		};
		visible = document.visibilityState === 'visible';
		document.addEventListener('visibilitychange', onVisibilityChange);
		return () => document.removeEventListener('visibilitychange', onVisibilityChange);
	});

	async function pay(method: Method) {
		if (connecting) return;
		connecting = method;
		failed = null;
		const seq = ++latest;
		try {
			const next = await snapApi.pay(token, method);
			if (seq === latest) snap = next;
		} catch (error) {
			const message = errorMessage(error);
			failed = { method, message };
			toast.error(message);
			// Another tab may already have chosen a method (409): show what the server holds now.
			await load(token, true);
		} finally {
			connecting = null;
		}
	}

	async function simulate(outcome: SimulateOutcome) {
		simulating = true;
		const seq = ++latest;
		try {
			const next = await snapApi.simulate(token, outcome);
			if (seq === latest) snap = next;
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			simulating = false;
		}
	}
</script>

<svelte:head>
	<title>{payment ? `Pay ${payment.order_id}` : 'Payment'} | PayRouters</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<SnapShell
	{merchant}
	livemode={payment?.livemode ?? null}
	{backUrl}
	loading={!snap && !notFound && !loadError}
>
	<div class="flex flex-1 flex-col gap-section" data-testid="page-snap">
		{#if notFound}
			<ResultHero
				tone="neutral"
				title="This payment link is not valid"
				detail="The link may have expired, been replaced or been mistyped. Ask the merchant for a new payment link."
				status="not_found"
				icon={Unlink}
			/>
		{:else if loadError && !snap}
			<ResultHero
				tone="neutral"
				title="Could not load this payment"
				detail={loadError}
				status="error"
			/>
			<Button size="lg" class="w-full" onclick={() => load(token, false)}>Try again</Button>
		{:else if !snap || !payment}
			<div
				class="flex flex-col gap-section"
				data-testid="snap-loading"
				role="status"
				aria-busy="true"
			>
				<span class="sr-only">Loading payment</span>
				<Skeleton class="h-4 w-24" />
				<Skeleton class="h-12 w-56" />
				<Skeleton class="h-4 w-48" />
				<Skeleton class="mt-2 h-5 w-44" />
				<Skeleton class="h-4 w-64" />
				<div class="overflow-hidden rounded-lg border border-border bg-card">
					{#each [0, 1, 2, 3, 4] as row (row)}
						<div class="flex items-center gap-3 border-b border-border p-4 last:border-b-0">
							<Skeleton class="size-11 shrink-0 rounded-md" />
							<div class="flex flex-1 flex-col gap-2">
								<Skeleton class="h-4 w-40" />
								<Skeleton class="h-3 w-24" />
							</div>
						</div>
					{/each}
				</div>
			</div>
		{:else if payment.status === 'pending' && payment.instructions}
			<InstructionsView
				{payment}
				instructions={payment.instructions}
				allowSimulate={snap.allow_simulate}
				{simulating}
				autoRedirect={!page.url.searchParams.has('paused')}
				onsimulate={simulate}
			/>
		{:else if payment.status === 'pending'}
			<AmountBlock amount={total} size="xl">
				{#snippet actions()}
					<PayBefore expiresAt={payment.expires_at} />
				{/snippet}
			</AmountBlock>
			<OrderMeta orderId={payment.order_id} description={payment.description} />
			<MethodPicker methods={snap.methods} {connecting} {failed} onpick={pay} />
		{:else}
			<PaymentResult {payment} merchantName={merchant ?? ''} />
		{/if}
	</div>
</SnapShell>
