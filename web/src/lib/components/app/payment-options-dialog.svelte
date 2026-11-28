<script lang="ts">
	/**
	 * Payment options of one product: the checklist of standard methods and hosted checkouts, each
	 * served by one provider. Loads GET payment-options when it opens and saves with PUT. Checking a
	 * method under one provider moves it away from the provider that served it before.
	 */
	import { toast } from 'svelte-sonner';
	import { resolve } from '$app/paths';
	import { adminApi } from '#lib/api/admin';
	import { errorMessage } from '#lib/api/client';
	import type {
		AvailableProvider,
		Mode,
		PaymentOption,
		PaymentOptionKey,
		PaymentOptions,
		Product,
		Provider
	} from '#lib/api/types';
	import { checkoutKey } from '#lib/payment-options';
	import { Resource } from '#lib/state/resource.svelte';
	import { Button } from '#lib/components/ui/button';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import Banner from '#lib/components/app/banner.svelte';
	import CheckOption from '#lib/components/app/check-option.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import Pill from '#lib/components/app/pill.svelte';
	import ResponsiveDialog from '#lib/components/app/responsive-dialog.svelte';

	let {
		open = $bindable(false),
		product,
		onsaved
	}: {
		open?: boolean;
		product: Product | null;
		/** Runs after a successful save, so the product list can be refreshed. */
		onsaved: () => void;
	} = $props();

	const MODES: Mode[] = ['sandbox', 'live'];
	const MODE_LABEL: Record<Mode, string> = { sandbox: 'Sandbox', live: 'Live' };

	const loaded = new Resource<PaymentOptions>();
	// The checklist as edited; order is the order the options were checked in.
	let picked = $state<PaymentOption[]>([]);
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	const productId = $derived(product?.id ?? null);
	const available = $derived<AvailableProvider[]>(loaded.data?.available ?? []);
	// Options whose provider no longer has an enabled account. They stay listed so they can be removed.
	const unavailable = $derived(
		picked.filter((o) => !available.some((p) => p.provider === o.provider))
	);
	const count = $derived(picked.length);

	$effect(() => {
		const id = productId;
		if (!open || !id) return;
		saveError = null;
		void loaded.load(() => adminApi.getPaymentOptions(id));
	});

	$effect(() => {
		const current = loaded.data;
		if (current) picked = [...current.data];
	});

	function labelOf(provider: Provider): string {
		return available.find((p) => p.provider === provider)?.label ?? provider;
	}

	function servedBy(option: PaymentOptionKey): Provider | undefined {
		return picked.find((o) => o.option === option)?.provider;
	}

	function setOption(option: PaymentOptionKey, provider: Provider, on: boolean) {
		const rest = picked.filter((o) => o.option !== option);
		picked = on ? [...rest, { option, provider }] : rest;
	}

	/** Shown under a method row: where it is served now, and what checking it here does. */
	function hintFor(option: PaymentOptionKey, provider: Provider): string | undefined {
		const owner = servedBy(option);
		if (owner === undefined) return undefined;
		if (owner === provider) return `Served by ${labelOf(provider)}`;
		return `Served by ${labelOf(owner)}. Checking it moves it to ${labelOf(provider)}.`;
	}

	function missingModes(provider: AvailableProvider): Mode[] {
		if (!picked.some((o) => o.provider === provider.provider)) return [];
		return MODES.filter((mode) => !provider.modes.includes(mode));
	}

	async function save() {
		const target = product;
		if (!target) return;
		saving = true;
		saveError = null;
		try {
			const saved = await adminApi.setPaymentOptions(target.id, picked);
			loaded.set(saved);
			open = false;
			toast.success('Payment options saved');
			onsaved();
		} catch (error) {
			saveError = errorMessage(error);
		} finally {
			saving = false;
		}
	}
</script>

<ResponsiveDialog
	bind:open
	size="lg"
	fullScreenOnPhone
	testid="dialog-payment-options"
	title="Payment options"
	description={product
		? `Pick the methods and checkouts ${product.name} offers. Each one is served by one provider.`
		: ''}
>
	{#if saveError}
		<Banner tone="error">{saveError}</Banner>
	{/if}

	{#if loaded.loading}
		<div class="flex flex-col gap-3" aria-busy="true" aria-label="Loading payment options">
			<Skeleton class="h-16 w-full rounded-md" />
			<Skeleton class="h-14 w-full rounded-md" />
			<Skeleton class="h-14 w-full rounded-md" />
		</div>
	{:else if loaded.error}
		<ErrorAlert error={loaded.error} subject="payment options" />
	{:else if loaded.data}
		<div
			class="flex flex-col gap-1 rounded-md bg-secondary p-3.5"
			data-testid="payment-options-state"
		>
			{#if count === 0}
				<span class="text-label text-foreground">Not configured: payments use one provider</span>
				<span class="text-helper text-muted-foreground">
					Without options, a request uses its provider, then the product default, then the first
					enabled provider.
				</span>
			{:else}
				<span class="tabular text-label text-foreground">
					{count}
					{count === 1 ? 'option' : 'options'} selected
				</span>
				<span class="text-helper text-muted-foreground">
					Payments without a provider are served from these options.
				</span>
			{/if}
		</div>

		{#if available.length === 0}
			<Banner tone="warn" title="No provider account is enabled">
				Add a provider account first, then choose its methods here.
				<a class="underline underline-offset-4" href={resolve('/(admin)/providers')}>
					Open Providers
				</a>
			</Banner>
		{/if}

		{#each available as provider (provider.provider)}
			<section class="flex flex-col gap-2.5" data-testid={`payment-provider-${provider.provider}`}>
				<div class="flex min-w-0 flex-wrap items-center gap-2">
					<h3 class="text-label break-words text-foreground">{provider.label}</h3>
					{#each MODES as mode (mode)}
						{#if provider.modes.includes(mode)}
							<Pill tone={mode === 'sandbox' ? 'sandbox' : 'success'}>{MODE_LABEL[mode]}</Pill>
						{/if}
					{/each}
				</div>

				{#each provider.methods as method (method.code)}
					<CheckOption
						testid={`payment-option-${method.code}`}
						label={method.label}
						helper={hintFor(method.code, provider.provider)}
						checked={servedBy(method.code) === provider.provider}
						onchange={(on: boolean) => setOption(method.code, provider.provider, on)}
					/>
				{/each}

				{#if provider.checkout}
					<CheckOption
						testid={`payment-option-checkout-${provider.provider}`}
						label={`${provider.label} checkout`}
						helper="Customers pay on the provider's own page."
						checked={servedBy(checkoutKey(provider.provider)) === provider.provider}
						onchange={(on: boolean) =>
							setOption(checkoutKey(provider.provider), provider.provider, on)}
					/>
				{/if}

				{#each missingModes(provider) as mode (mode)}
					<Banner tone="warn">
						No {mode} account for {provider.label}. {MODE_LABEL[mode]} payments skip its options.
					</Banner>
				{/each}
			</section>
		{/each}

		{#if unavailable.length > 0}
			<section class="flex flex-col gap-2.5" data-testid="payment-options-unavailable">
				<h3 class="text-label text-foreground">Without an enabled account</h3>
				{#each unavailable as entry (entry.option)}
					<CheckOption
						label={entry.option}
						helper={`${labelOf(entry.provider)} has no enabled account. Uncheck to remove it.`}
						checked
						onchange={(on: boolean) => setOption(entry.option, entry.provider, on)}
					/>
				{/each}
			</section>
		{/if}
	{/if}

	{#snippet footer()}
		<Button type="button" variant="outline" class="w-full sm:w-auto" onclick={() => (open = false)}>
			Cancel
		</Button>
		<Button
			type="button"
			class="w-full sm:w-auto"
			disabled={saving || loaded.loading || !loaded.data}
			onclick={save}
		>
			{saving ? 'Saving…' : 'Save'}
		</Button>
	{/snippet}
</ResponsiveDialog>
