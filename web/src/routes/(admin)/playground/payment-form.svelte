<script lang="ts">
	import type { Meta, Mode, Method, Product, Provider, ProviderAccount } from '#lib/api/types';
	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import FormField from '#lib/components/app/form-field.svelte';
	import SelectField from '#lib/components/app/select-field.svelte';
	import ErrorAlert from '#lib/components/app/error-alert.svelte';
	import Pill from '#lib/components/app/pill.svelte';
	import ProviderOptionsFields from './provider-options-fields.svelte';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import ChevronUp from '@lucide/svelte/icons/chevron-up';
	import Send from '@lucide/svelte/icons/send';
	import Shuffle from '@lucide/svelte/icons/shuffle';
	import { formatIdr } from '#lib/format';
	import { cn } from '#lib/utils';
	import {
		IDRX_MIN_AMOUNT,
		MIN_EXPIRES_IN,
		ORDER_ID_MAX,
		blankDraft,
		formatAmountInput,
		newOrderId,
		validateDraft,
		type DraftErrors,
		type DraftField,
		type PaymentDraft
	} from './draft';

	/**
	 * The "New payment" form. A message shows once its field was left or a submit was attempted.
	 * On phones the submit row is a sticky bar above the tab bar; from md it sits at the end of the
	 * card. Both layouts are one element, so the test hook is unique.
	 */
	let {
		mode,
		products,
		accounts,
		meta,
		submitting,
		error,
		onsubmit
	}: {
		mode: Mode;
		products: Product[];
		accounts: ProviderAccount[];
		meta: Meta | undefined;
		submitting: boolean;
		error: unknown;
		onsubmit: (draft: PaymentDraft) => void;
	} = $props();

	let draft = $state<PaymentDraft>(blankDraft());
	let touched = $state<Partial<Record<DraftField, boolean>>>({});
	let attempted = $state(false);
	let advancedOpen = $state(false);

	const errors: DraftErrors = $derived(validateDraft(draft));
	const invalid = $derived(Object.values(errors).some(Boolean));
	const shown = (field: DraftField): string | undefined =>
		touched[field] || attempted ? errors[field] : undefined;
	const advancedHasError = $derived(
		!!shown('expiresIn') || !!shown('returnUrl') || !!shown('idrxWallet')
	);
	const showAdvanced = $derived(advancedOpen || advancedHasError);

	const touch = (field: DraftField) => (touched = { ...touched, [field]: true });

	const productOptions = $derived([
		{ value: '', label: 'Choose a product' },
		...products.map((p) => ({ value: p.id, label: p.name }))
	]);
	const providerOptions = $derived([
		{ value: '', label: 'Automatic' },
		...accounts
			.filter((a) => a.mode === mode && a.enabled)
			.map((a) => ({ value: a.provider, label: a.label }))
	]);

	/** Methods offered for a provider ('' = every provider's methods, as the Snap page sees them). */
	function methodsFor(provider: Provider | ''): { code: Method; label: string }[] {
		if (provider === '') return meta?.methods ?? [];
		return meta?.providers.find((p) => p.id === provider)?.methods ?? [];
	}
	const methodsForProvider = $derived(methodsFor(draft.provider));
	const methodOptions = $derived([
		{ value: '', label: 'Customer chooses (Snap)' },
		...methodsForProvider.map((m) => ({ value: m.code, label: m.label }))
	]);

	const checkoutChoices = [
		{
			value: 'snap',
			title: 'PayRouters Snap (core API)',
			sub: 'Customer picks the method on our page'
		},
		{
			value: 'provider',
			title: 'Provider-hosted page',
			sub: 'Midtrans Snap, Xendit or IDRX checkout'
		}
	] as const;

	function chooseCheckout(value: PaymentDraft['checkout']) {
		draft.checkout = value;
		// The method only applies to PayRouters Snap; provider-hosted checkout chooses its own.
		if (value === 'provider') draft.method = '';
	}

	function chooseProvider(value: string) {
		draft.provider = value as PaymentDraft['provider'];
		// A method the chosen provider does not offer is cleared rather than sent.
		if (draft.method !== '' && !methodsFor(draft.provider).some((m) => m.code === draft.method))
			draft.method = '';
	}

	// The mode switch can take a selected provider out of the list; reconcile that one case.
	$effect(() => {
		if (draft.provider !== '' && !providerOptions.some((o) => o.value === draft.provider))
			draft.provider = '';
	});

	function onAmountInput(event: Event & { currentTarget: HTMLInputElement }) {
		const digits = event.currentTarget.value.replace(/\D/g, '');
		draft.amount = digits;
		event.currentTarget.value = formatAmountInput(digits);
	}

	const amountHelper = $derived(
		draft.provider === 'idrx'
			? `IDRX mints at least ${formatIdr(IDRX_MIN_AMOUNT)}.`
			: 'Whole rupiah. Minimum Rp 1.'
	);

	function submit(event: SubmitEvent) {
		event.preventDefault();
		attempted = true;
		if (invalid) return;
		onsubmit($state.snapshot(draft));
	}
</script>

<form
	class="flex flex-col gap-5 pb-28 md:pb-0"
	onsubmit={submit}
	data-testid="playground-form"
	novalidate
>
	<div class="grid gap-5 md:grid-cols-2 md:gap-x-4">
		<FormField id="pg-product" label="Product" error={shown('productId')}>
			<SelectField
				bind:value={() => draft.productId, (v: string) => (draft.productId = v)}
				options={productOptions}
				label="Choose a product"
				class="w-full"
			/>
		</FormField>

		<FormField
			id="pg-mode"
			label="Mode"
			helper={mode === 'sandbox'
				? 'Follows the top bar switch.'
				: 'Follows the top bar switch. Live mode creates real payments.'}
		>
			<div
				class="flex h-control items-center justify-between rounded-md border border-input bg-secondary px-3.5 text-body"
				data-testid="mode-readout"
				data-mode={mode}
			>
				<span>{mode === 'sandbox' ? 'Sandbox' : 'Live'}</span>
				<Pill tone={mode === 'sandbox' ? 'sandbox' : 'neutral'}>
					{mode === 'sandbox' ? 'Sandbox' : 'Live'}
				</Pill>
			</div>
		</FormField>

		<fieldset class="flex flex-col gap-2 md:col-span-2">
			<legend class="mb-2 text-label">Checkout</legend>
			<div class="grid gap-2 md:grid-cols-2 md:gap-x-4">
				{#each checkoutChoices as choice (choice.value)}
					<label
						class="flex cursor-pointer items-start gap-3 rounded-md border border-input bg-card p-3.5 has-checked:ring-2 has-checked:ring-primary has-focus-visible:ring-2 has-focus-visible:ring-ring/50"
						data-testid={`checkout-${choice.value}`}
					>
						<input
							type="radio"
							name="checkout"
							value={choice.value}
							checked={draft.checkout === choice.value}
							onchange={() => chooseCheckout(choice.value)}
							class="peer sr-only"
						/>
						<span
							class="mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full border border-input bg-card peer-checked:border-primary after:size-2 after:rounded-full after:bg-primary after:opacity-0 peer-checked:after:opacity-100"
							aria-hidden="true"
						></span>
						<span class="flex min-w-0 flex-col gap-0.5">
							<span class="text-label">{choice.title}</span>
							<span class="text-helper text-muted-foreground">{choice.sub}</span>
						</span>
					</label>
				{/each}
			</div>
		</fieldset>

		<FormField
			id="pg-provider"
			label="Provider"
			helper="Automatic picks the first enabled account for this mode."
		>
			<SelectField
				bind:value={() => draft.provider, chooseProvider}
				options={providerOptions}
				label="Provider"
				class="w-full"
			/>
		</FormField>

		<FormField
			id="pg-method"
			label="Method"
			helper={draft.checkout === 'provider' ? 'Disabled for provider-hosted checkout.' : undefined}
		>
			<SelectField
				bind:value={() => draft.method, (v: string) => (draft.method = v as PaymentDraft['method'])}
				options={methodOptions}
				label="Method"
				class={cn('w-full', draft.checkout === 'provider' && 'pointer-events-none opacity-50')}
			/>
		</FormField>

		<FormField
			id="pg-amount"
			label="Amount"
			error={shown('amount')}
			helper={shown('amount') ? undefined : amountHelper}
		>
			<div class="relative">
				<span
					class="pointer-events-none absolute inset-y-0 left-0 flex items-center border-r border-input pr-3 pl-3.5 text-body text-muted-foreground"
					aria-hidden="true"
				>
					Rp
				</span>
				<Input
					id="pg-amount"
					inputmode="numeric"
					autocomplete="off"
					class="tabular pl-14"
					value={formatAmountInput(draft.amount)}
					oninput={onAmountInput}
					onblur={() => touch('amount')}
					aria-invalid={shown('amount') ? true : undefined}
				/>
			</div>
		</FormField>

		<FormField
			id="pg-order"
			label="Order ID"
			error={shown('orderId')}
			helper={shown('orderId') ? undefined : 'Regenerate for a new ID.'}
		>
			<div class="flex gap-2">
				<Input
					id="pg-order"
					class="min-w-0 flex-1 font-mono"
					maxlength={ORDER_ID_MAX}
					autocomplete="off"
					spellcheck={false}
					bind:value={draft.orderId}
					onblur={() => touch('orderId')}
					aria-invalid={shown('orderId') ? true : undefined}
				/>
				<Button
					type="button"
					variant="outline"
					size="icon"
					aria-label="New order ID"
					onclick={() => (draft.orderId = newOrderId())}
				>
					<Shuffle />
				</Button>
			</div>
		</FormField>

		<FormField id="pg-name" label="Customer name" optional>
			<Input id="pg-name" autocomplete="off" bind:value={draft.customerName} />
		</FormField>
		<FormField id="pg-email" label="Email" optional error={shown('customerEmail')}>
			<Input
				id="pg-email"
				type="email"
				autocomplete="off"
				bind:value={draft.customerEmail}
				onblur={() => touch('customerEmail')}
				aria-invalid={shown('customerEmail') ? true : undefined}
			/>
		</FormField>

		<FormField id="pg-phone" label="Phone" optional error={shown('customerPhone')}>
			<Input
				id="pg-phone"
				type="tel"
				placeholder="+628123456789"
				autocomplete="off"
				bind:value={draft.customerPhone}
				onblur={() => touch('customerPhone')}
				aria-invalid={shown('customerPhone') ? true : undefined}
			/>
		</FormField>
		<FormField id="pg-desc" label="Description" optional>
			<Input id="pg-desc" placeholder="Order #1" bind:value={draft.description} />
		</FormField>
	</div>

	<div class="flex flex-col gap-4 border-t pt-5">
		<button
			type="button"
			class="flex min-h-touch items-center justify-between gap-2 rounded-md text-left text-label"
			aria-expanded={showAdvanced}
			aria-controls="pg-advanced"
			onclick={() => (advancedOpen = !advancedOpen)}
			data-testid="advanced-toggle"
		>
			Advanced options
			{#if showAdvanced}<ChevronUp class="size-4" />{:else}<ChevronDown class="size-4" />{/if}
		</button>

		{#if showAdvanced}
			<div id="pg-advanced" class="grid gap-5 md:grid-cols-2 md:gap-x-4">
				<FormField
					id="pg-expires"
					label="Expires in (seconds)"
					optional
					error={shown('expiresIn')}
					helper={shown('expiresIn')
						? undefined
						: `Default 86400 (24 h). Minimum ${MIN_EXPIRES_IN}.`}
				>
					<Input
						id="pg-expires"
						inputmode="numeric"
						autocomplete="off"
						placeholder="86400"
						bind:value={draft.expiresIn}
						onblur={() => touch('expiresIn')}
						aria-invalid={shown('expiresIn') ? true : undefined}
					/>
				</FormField>
				<FormField
					id="pg-return"
					label="Return URL"
					optional
					error={shown('returnUrl')}
					helper={shown('returnUrl') ? undefined : 'Where the customer goes after paying.'}
				>
					<Input
						id="pg-return"
						type="url"
						placeholder="https://app.example.com/thanks"
						autocomplete="off"
						bind:value={draft.returnUrl}
						onblur={() => touch('returnUrl')}
						aria-invalid={shown('returnUrl') ? true : undefined}
					/>
				</FormField>
				{#if draft.provider === 'idrx'}
					<div class="md:col-span-2">
						<ProviderOptionsFields
							bind:wallet={draft.idrxWallet}
							bind:chain={
								() => draft.idrxChain,
								(v: string) => (draft.idrxChain = v as PaymentDraft['idrxChain'])
							}
							walletError={shown('idrxWallet')}
						/>
					</div>
				{/if}
			</div>
		{/if}
	</div>

	<div
		class="flex flex-col gap-3 border-t pt-4 max-md:fixed max-md:inset-x-0 max-md:bottom-[88px] max-md:z-20 max-md:bg-card max-md:px-page max-md:pt-3 max-md:pb-3"
		data-testid="create-bar"
	>
		<ErrorAlert {error} />
		<Button
			type="submit"
			class="w-full"
			disabled={submitting}
			aria-busy={submitting}
			data-testid="create-payment"
		>
			<Send data-icon="inline-start" />
			Create payment
		</Button>
		<p class="text-center text-helper text-muted-foreground">
			{mode === 'sandbox'
				? 'Creates a sandbox payment. Nothing is charged.'
				: 'Creates a live payment. The customer can pay it for real.'}
		</p>
	</div>
</form>
