<script lang="ts">
	/**
	 * Credential inputs for one provider, driven by the server's `credential_fields`. Secrets are
	 * write-only (`SecretInput` with the saved mask), the chain id is a select, the rest are text.
	 */
	import type { CredentialField } from '#lib/api/types';
	import { CHAIN_OPTIONS, CREDENTIAL_HELPERS } from '#lib/provider-display';
	import FormField from '#lib/components/app/form-field.svelte';
	import SecretInput from '#lib/components/app/secret-input.svelte';
	import SelectField from '#lib/components/app/select-field.svelte';
	import { Input } from '#lib/components/ui/input';

	let {
		fields,
		values = $bindable(),
		saved = {},
		errors = {}
	}: {
		fields: CredentialField[];
		/** Plain values by key. Secret keys hold what the user typed (blank keeps the saved secret). */
		values: Record<string, string>;
		/** Masked secrets from the account, e.g. { server_key: "••••d1e4" }. */
		saved?: Record<string, string>;
		errors?: Record<string, string>;
	} = $props();
</script>

{#each fields as field (field.key)}
	{@const id = `cred-${field.key}`}
	{@const savedMask = field.secret ? (saved[field.key] ?? '') : ''}
	{@const helper =
		field.secret && savedMask
			? 'Leave blank to keep the saved key'
			: (CREDENTIAL_HELPERS[field.key] ?? null)}
	<FormField
		{id}
		label={field.label}
		optional={!field.required}
		{helper}
		error={errors[field.key] ?? null}
	>
		{#if field.secret}
			<SecretInput
				{id}
				bind:value={
					() => (values[field.key] as string | undefined) ?? '',
					(v: string) => (values[field.key] = v)
				}
				saved={savedMask || null}
				placeholder={field.placeholder ?? ''}
				invalid={!!errors[field.key]}
				describedBy={helper ? `${id}-helper` : undefined}
			/>
		{:else if field.key === 'default_chain_id'}
			<SelectField
				bind:value={
					() => (values[field.key] as string | undefined) ?? '',
					(v: string) => (values[field.key] = v)
				}
				options={CHAIN_OPTIONS}
				label={field.label}
				class="w-full"
			/>
		{:else}
			<Input
				{id}
				autocomplete="off"
				spellcheck={false}
				class={field.key === 'default_wallet_address' ? 'font-mono' : ''}
				placeholder={field.placeholder ?? ''}
				aria-invalid={!!errors[field.key] || undefined}
				bind:value={
					() => (values[field.key] as string | undefined) ?? '',
					(v: string) => (values[field.key] = v)
				}
			/>
		{/if}
	</FormField>
{/each}
