<script lang="ts">
	/**
	 * Name, webhook URL and default provider, shared by New product and Edit product. Errors show
	 * after the first submit and update as the user types. `onvalid` runs with the trimmed-ready values.
	 */
	import { validateProductForm, type ProductFormValues } from '#lib/product-form';
	import FormField from '#lib/components/app/form-field.svelte';
	import SelectField from '#lib/components/app/select-field.svelte';
	import { Input } from '#lib/components/ui/input';

	let {
		id,
		values = $bindable(),
		providerOptions,
		onvalid
	}: {
		/** Prefix for the field ids and the form id (the dialog's submit button targets it). */
		id: string;
		values: ProductFormValues;
		providerOptions: { value: string; label: string }[];
		onvalid: (values: ProductFormValues) => void;
	} = $props();

	let submitted = $state(false);
	const errors = $derived(submitted ? validateProductForm(values) : {});

	function submit(event: SubmitEvent) {
		event.preventDefault();
		submitted = true;
		if (Object.keys(validateProductForm(values)).length === 0) onvalid(values);
	}
</script>

<form {id} class="flex flex-col gap-4" novalidate onsubmit={submit}>
	<FormField id="{id}-name" label="Name" error={errors.name}>
		<Input
			id="{id}-name"
			autocomplete="off"
			maxlength={120}
			placeholder="Demo Shop"
			aria-invalid={!!errors.name || undefined}
			aria-describedby={errors.name ? `${id}-name-error` : undefined}
			bind:value={values.name}
		/>
	</FormField>

	<FormField
		id="{id}-webhook"
		label="Webhook URL"
		optional
		error={errors.webhookUrl}
		helper="We POST signed payment events here. You can add it later."
	>
		<Input
			id="{id}-webhook"
			type="url"
			inputmode="url"
			autocomplete="off"
			placeholder="https://api.example.com/payrouters/webhook"
			aria-invalid={!!errors.webhookUrl || undefined}
			aria-describedby={errors.webhookUrl ? `${id}-webhook-error` : undefined}
			bind:value={values.webhookUrl}
		/>
	</FormField>

	<FormField
		id="{id}-provider"
		label="Default provider"
		optional
		helper="Used when a payment request does not name a provider and the product has no payment options."
	>
		<SelectField
			bind:value={values.defaultProvider}
			options={providerOptions}
			label="Default provider"
			class="w-full"
		/>
	</FormField>
</form>
