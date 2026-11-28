// Helpers for a product's payment options: the checklist of standard methods and hosted checkouts.
import type { PaymentOption, PaymentOptionKey, Provider } from '#lib/api/types';
import { shortMethodLabel } from '#lib/provider-display';

/** The option key of a provider's hosted checkout page, e.g. `checkout:paypal`. */
export function checkoutKey(provider: Provider): PaymentOptionKey {
	return `checkout:${provider}`;
}

/** What one provider serves for a product: short chips such as "QRIS", "BCA VA", "Hosted checkout". */
export interface ProviderOptionGroup {
	provider: Provider;
	label: string;
	items: { key: PaymentOptionKey; label: string; checkout: boolean }[];
}

/**
 * A product's options grouped by provider, in the order each provider first appears. Short labels,
 * because the provider is already named by the group: "QRIS" under Midtrans, not "QRIS (Midtrans)".
 */
export function groupPaymentOptions(
	options: PaymentOption[],
	providers: Record<string, string>,
	methods: Record<string, string>
): ProviderOptionGroup[] {
	const groups: ProviderOptionGroup[] = [];
	for (const option of options) {
		let group = groups.find((g) => g.provider === option.provider);
		if (!group) {
			group = {
				provider: option.provider,
				label: providers[option.provider] ?? option.provider,
				items: []
			};
			groups.push(group);
		}
		const checkout = option.option.startsWith('checkout:');
		group.items.push({
			key: option.option,
			label: checkout
				? 'Hosted checkout'
				: shortMethodLabel(methods[option.option] ?? option.option),
			checkout
		});
	}
	return groups;
}
