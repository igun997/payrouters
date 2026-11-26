<script lang="ts">
	/**
	 * Provider account form, shared by Add (create) and Edit. Create picks the provider and mode;
	 * a provider with no sandbox is locked to live and warns. Edit shows the inbound webhook URL,
	 * the enabled switch and locked provider and mode. Credentials and fees use the same inputs.
	 */
	import { untrack } from 'svelte';
	import type { MetaProvider, ProviderAccount } from '#lib/api/types';
	import CopyField from '#lib/components/app/copy-field.svelte';
	import CredentialFields from '#lib/components/app/credential-fields.svelte';
	import FeeEditor from '#lib/components/app/fee-editor.svelte';
	import FormField from '#lib/components/app/form-field.svelte';
	import RadioCards from '#lib/components/app/radio-cards.svelte';
	import SelectField from '#lib/components/app/select-field.svelte';
	import Banner from '#lib/components/app/banner.svelte';
	import { PASTE_HINT } from '#lib/provider-display';
	import {
		defaultCredentials,
		NO_PROVIDER_ERRORS,
		validateProviderForm,
		type ProviderFormValues
	} from '#lib/provider-form';
	import { Input } from '#lib/components/ui/input';
	import { Switch } from '#lib/components/ui/switch';
	import Lock from '@lucide/svelte/icons/lock';

	let {
		id,
		mode,
		account,
		meta,
		values = $bindable(),
		onvalid
	}: {
		/** Form id; the dialog's submit button targets it. */
		id: string;
		mode: 'create' | 'edit';
		account: ProviderAccount | null;
		meta: MetaProvider[];
		values: ProviderFormValues;
		onvalid: (values: ProviderFormValues) => void;
	} = $props();

	const selected = $derived(meta.find((p) => p.id === values.provider));
	const sandboxLocked = $derived(selected !== undefined && !selected.supports_sandbox);
	const providerOptions = $derived(meta.map((p) => ({ value: p.id, label: p.label })));
	const feeMethods = $derived([
		{ value: '*', label: 'Default (*)' },
		...(selected?.methods ?? []).map((m) => ({ value: m.code, label: m.label }))
	]);
	const modeOptions = [
		{ value: 'sandbox' as const, label: 'Sandbox', description: 'Test keys, no real money' },
		{ value: 'live' as const, label: 'Live', description: 'Real money, production keys' }
	];

	// Changing the provider in create resets what depends on it: mode, credentials and fees.
	$effect(() => {
		const info = meta.find((p) => p.id === values.provider);
		if (mode !== 'create' || !info) return;
		untrack(() => {
			values.mode = info.supports_sandbox ? 'sandbox' : 'live';
			values.credentials = defaultCredentials(info.credential_fields);
			values.fees = [];
		});
	});

	let submitted = $state(false);
	const errors = $derived(
		submitted ? validateProviderForm(values, mode, selected) : NO_PROVIDER_ERRORS
	);

	function submit(event: SubmitEvent) {
		event.preventDefault();
		submitted = true;
		const next = validateProviderForm(values, mode, selected);
		if (!next.provider && Object.keys(next.credentials).length === 0) onvalid(values);
	}
</script>

<form {id} class="flex flex-col gap-5" novalidate onsubmit={submit}>
	{#if mode === 'edit' && account}
		<CopyField
			label="Inbound webhook URL"
			value={account.webhook_url}
			helper={PASTE_HINT[account.provider] ?? undefined}
		/>

		<div class="flex items-center justify-between gap-4 rounded-md border p-3.5">
			<div class="flex min-w-0 flex-col gap-0.5">
				<label for="{id}-enabled" class="text-label text-foreground">Enabled</label>
				<span class="text-helper text-muted-foreground">
					New payments can route to this account while it is on
				</span>
			</div>
			<Switch id="{id}-enabled" bind:checked={values.enabled} />
		</div>
	{/if}

	<FormField id="{id}-provider" label="Provider" error={errors.provider ?? null}>
		{#if mode === 'create'}
			<SelectField
				bind:value={values.provider}
				options={providerOptions}
				label="Choose provider"
				class="w-full"
			/>
		{:else}
			<Input id="{id}-provider" value={selected?.label ?? account?.provider ?? ''} disabled />
		{/if}
	</FormField>

	{#if mode === 'create' && selected && !sandboxLocked}
		<RadioCards
			name="{id}-mode"
			legend="Mode"
			testid="provider-mode"
			options={modeOptions}
			bind:value={values.mode}
		/>
	{:else}
		<div class="flex flex-col gap-1.5">
			<span class="text-label text-foreground">Mode</span>
			<div class="relative">
				<Input
					value={sandboxLocked || account?.mode === 'live' ? 'Live' : 'Sandbox'}
					disabled
					class="pe-10"
					aria-label="Mode (locked)"
				/>
				<Lock
					class="pointer-events-none absolute top-1/2 right-3 size-4 -translate-y-1/2 text-muted-foreground"
				/>
			</div>
			<p class="text-helper text-muted-foreground">
				{#if sandboxLocked && selected}
					{selected.label} has no sandbox environment. This account is always live.
				{:else}
					Mode cannot change. Create a new account for live.
				{/if}
			</p>
		</div>
	{/if}

	{#if sandboxLocked && selected && mode === 'create'}
		<Banner tone="warn" title="No sandbox: every payment moves real money" />
	{/if}

	<FormField id="{id}-label" label="Label" optional>
		<Input
			id="{id}-label"
			maxlength={120}
			autocomplete="off"
			placeholder={selected ? `${selected.label} ${values.mode}` : 'Label'}
			bind:value={values.label}
		/>
	</FormField>

	{#if selected}
		<section class="flex flex-col gap-3">
			<div class="flex flex-col gap-0.5">
				<h3 class="text-label text-foreground">Credentials</h3>
				<p class="text-helper text-muted-foreground">
					{mode === 'edit'
						? 'Secrets stay masked. Leave one blank to keep the saved value.'
						: 'Stored encrypted. Shown masked after you save.'}
				</p>
			</div>
			<CredentialFields
				fields={selected.credential_fields}
				bind:values={values.credentials}
				saved={mode === 'edit' && account ? account.credentials : {}}
				errors={errors.credentials}
			/>
		</section>

		<FeeEditor bind:rows={values.fees} methods={feeMethods} />
	{/if}
</form>
