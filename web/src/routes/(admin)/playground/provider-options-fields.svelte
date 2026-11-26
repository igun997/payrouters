<script lang="ts">
	import FormField from '#lib/components/app/form-field.svelte';
	import SelectField from '#lib/components/app/select-field.svelte';
	import { Input } from '#lib/components/ui/input';

	/**
	 * IDRX-only provider options (`provider_options`). Empty fields fall back to the provider
	 * account defaults, so both are optional.
	 */
	let {
		wallet = $bindable(''),
		chain = $bindable(''),
		walletError
	}: { wallet?: string; chain?: string; walletError?: string } = $props();

	const chainOptions = [
		{ value: '', label: 'Account default' },
		{ value: '8453', label: 'Base (8453)' }
	];
</script>

<div class="flex flex-col gap-4 rounded-md border bg-secondary p-4" data-testid="idrx-options">
	<div class="flex flex-col gap-1">
		<h3 class="text-label">IDRX provider options</h3>
		<p class="text-helper text-muted-foreground">
			Only used when the provider is IDRX. Empty fields fall back to the account defaults.
		</p>
	</div>
	<FormField id="pg-idrx-wallet" label="Destination wallet" optional error={walletError}>
		<Input
			id="pg-idrx-wallet"
			class="font-mono"
			placeholder="0x…"
			autocomplete="off"
			spellcheck={false}
			aria-invalid={walletError ? true : undefined}
			bind:value={wallet}
		/>
	</FormField>
	<FormField id="pg-idrx-chain" label="Chain" optional>
		<SelectField bind:value={chain} options={chainOptions} label="Chain" class="w-full" />
	</FormField>
</div>
