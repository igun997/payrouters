export type ProductFormValues = { name: string; webhookUrl: string; defaultProvider: string };

export type ProductFormErrors = Partial<Record<keyof ProductFormValues, string>>;

export function blankProductForm(): ProductFormValues {
	return { name: '', webhookUrl: '', defaultProvider: '' };
}

/** Client-side rules. An empty webhook is allowed (no deliveries). */
export function validateProductForm(values: ProductFormValues): ProductFormErrors {
	const errors: ProductFormErrors = {};
	if (!values.name.trim()) errors.name = 'Enter a name for the product';
	const webhook = values.webhookUrl.trim();
	if (webhook && !webhook.startsWith('https://'))
		errors.webhookUrl = 'Webhook URL must start with https://';
	return errors;
}
