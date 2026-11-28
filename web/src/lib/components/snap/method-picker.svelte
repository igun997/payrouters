<script lang="ts">
	/*
	 * Choose-how-to-pay state: QRIS first, then bank transfer rows, then the provider's own checkout
	 * pages. The page owns the request; this only renders.
	 */
	import CheckoutRow from './checkout-row.svelte';
	import MethodRow from './method-row.svelte';
	import { choiceKey } from './method-meta';
	import type { Method, PayChoice, Provider, SnapCheckout, SnapMethod } from '#lib/api/types';

	let {
		methods,
		checkouts,
		connecting,
		failed,
		onpick
	}: {
		methods: SnapMethod[];
		checkouts: SnapCheckout[];
		/** Key of the choice whose request is in flight (see choiceKey). */
		connecting: string | null;
		/** Last choice the provider refused to start, by its key. */
		failed: { key: string } | null;
		onpick: (choice: PayChoice) => void;
	} = $props();

	const groups = $derived(
		[
			{ title: 'Scan to pay', items: methods.filter((m) => m.group === 'qris') },
			{ title: 'Bank transfer', items: methods.filter((m) => m.group === 'virtual_account') }
		].filter((group) => group.items.length > 0)
	);

	function rowState(key: string) {
		if (connecting === key) return 'connecting' as const;
		if (failed?.key === key) return 'error' as const;
		return 'idle' as const;
	}
</script>

<section class="flex flex-col gap-section" data-testid="snap-methods">
	<header class="flex flex-col gap-1">
		<h1 class="text-page-title font-extrabold">Choose how to pay</h1>
		<p class="text-body text-muted-foreground">
			You will see the account, QR code or provider page on the next step
		</p>
	</header>

	{#each groups as group (group.title)}
		<div class="flex flex-col gap-3 sm:gap-2">
			<h2 class="text-micro text-muted-foreground">{group.title}</h2>
			<ul class="flex flex-col gap-3 sm:gap-2">
				{#each group.items as method (method.code)}
					<li class="overflow-hidden rounded-lg border border-border bg-card">
						<MethodRow
							{method}
							state={rowState(choiceKey({ method: method.code }))}
							disabled={connecting !== null}
							onpick={(code: Method) => onpick({ method: code })}
						/>
					</li>
				{/each}
			</ul>
		</div>
	{/each}

	{#if checkouts.length > 0}
		<div class="flex flex-col gap-3 sm:gap-2" data-testid="snap-checkouts">
			<h2 class="text-micro text-muted-foreground">Provider checkout</h2>
			<ul class="flex flex-col gap-3 sm:gap-2">
				{#each checkouts as checkout (checkout.provider)}
					<li class="overflow-hidden rounded-lg border border-border bg-card">
						<CheckoutRow
							{checkout}
							state={rowState(choiceKey({ provider: checkout.provider }))}
							disabled={connecting !== null}
							onpick={(provider: Provider) => onpick({ provider })}
						/>
					</li>
				{/each}
			</ul>
		</div>
	{/if}

	{#if groups.length === 0 && checkouts.length === 0}
		<p class="py-4 text-center text-body text-muted-foreground">
			No payment methods are available for this order right now.
		</p>
	{/if}
</section>
