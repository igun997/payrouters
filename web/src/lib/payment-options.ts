// Helpers for a product's payment options: the checklist of standard methods and hosted checkouts.
import type { PaymentOption, PaymentOptionKey, Provider } from '#lib/api/types';

/** The option key of a provider's hosted checkout page, e.g. `checkout:paypal`. */
export function checkoutKey(provider: Provider): PaymentOptionKey {
	return `checkout:${provider}`;
}

/** One option as text: "QRIS (Midtrans)" for a method, "PayPal checkout" for a hosted checkout. */
export function optionLabel(
	option: PaymentOption,
	providers: Record<string, string>,
	methods: Record<string, string>
): string {
	const provider = providers[option.provider] ?? option.provider;
	if (option.option.startsWith('checkout:')) return `${provider} checkout`;
	return `${methods[option.option] ?? option.option} (${provider})`;
}

/** Product card summary: "2 options: QRIS (Midtrans), PayPal checkout", or "Single provider" when none are set. */
export function summarizePaymentOptions(
	options: PaymentOption[],
	providers: Record<string, string>,
	methods: Record<string, string>
): string {
	if (options.length === 0) return 'Single provider';
	const items = options.map((option) => optionLabel(option, providers, methods)).join(', ');
	return `${options.length} ${options.length === 1 ? 'option' : 'options'}: ${items}`;
}
