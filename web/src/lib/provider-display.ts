/** Display helpers shared by the provider cards, the account form and the fee rows. */
import type { FeeRule, Provider } from '#lib/api/types';
import { formatIdr } from '#lib/format';

/** Fee rows use short method names: "BCA Virtual Account" → "BCA VA". Other labels pass through. */
export function shortMethodLabel(label: string): string {
	return label.replace(' Virtual Account', ' VA');
}

/** Where each provider expects the PayRouters inbound webhook URL to be pasted. */
export const PASTE_HINT: Record<string, string> = {
	midtrans: 'Paste into Midtrans → Settings → Configuration → Payment Notification URL.',
	xendit: 'Paste into Xendit → Settings → Webhooks, for payment and payment session events.',
	idrx: 'Paste into IDRX → API Key → Callback URL. Callbacks are unsigned, so each one is re-checked with IDRX.',
	mayar:
		'Paste into Mayar → Integration → Webhook. Mayar allows one URL per account. Each event is re-checked with Mayar; paste the Webhook Token below as well to refuse requests that do not carry it.',
	paypal:
		'Add as a webhook URL on your PayPal app (Developer → Apps & Credentials) for Checkout order approved and Payment capture events, then copy the Webhook ID into this account.'
};

/** Base, Polygon and BNB are the chains IDRX settles on. The default is Base. */
export const CHAIN_OPTIONS = [
	{ value: '8453', label: 'Base (8453)' },
	{ value: '137', label: 'Polygon (137)' },
	{ value: '56', label: 'BNB (56)' }
];

export const DEFAULT_CHAIN_ID = '8453';

/** Helper text for credential keys that need one beyond their placeholder. */
export const CREDENTIAL_HELPERS: Record<string, string> = {
	default_wallet_address:
		'Used when a payment does not set provider_options.destination_wallet_address.',
	default_chain_id:
		'Used when a payment omits provider_options.chain_id. Options: Polygon (137), BNB (56).',
	idr_rate:
		'Rupiah per US dollar. Leave blank to charge at the European Central Bank’s daily reference rate (official, mid-market, no PayPal spread). Set a number to fix the rate yourself, for example to build in a margin. PayPal always charges in USD: customers are charged amount ÷ rate dollars, rounded up to the next cent.',
	webhook_token:
		'Optional. Mayar → Integration → Webhook Token. Mayar does not say which header carries it, so any header is accepted; if Mayar sends it elsewhere, webhooks are refused (payments still settle by polling), so clear it.',
	webhook_id:
		'Created when you add this account’s webhook URL on PayPal. Without it PayPal webhooks are refused; payments still settle when the payer returns and by polling.'
};

/** Two-letter tile code for a provider. */
export const PROVIDER_CODE: Record<string, string> = {
	midtrans: 'MT',
	xendit: 'XD',
	idrx: 'ID',
	mayar: 'MY',
	paypal: 'PP',
	mock: 'MK'
};

/** One line for the card: "Rp 4.000 on BCA VA, 0,7% on every method". */
export function feeSummary(
	provider: Provider,
	fees: Record<string, FeeRule>,
	methodLabels: Record<string, string>
): string {
	const rules = Object.entries(fees);
	if (rules.length === 0)
		return provider === 'mock' ? 'None' : 'Reported by the provider per payment';
	return rules
		.map(([method, rule]) => {
			const parts = [
				rule.fixed > 0 ? formatIdr(rule.fixed) : null,
				rule.percent > 0
					? `${rule.percent.toLocaleString('id-ID', { maximumFractionDigits: 2 })}%`
					: null
			].filter((part): part is string => part !== null);
			const amount = parts.length > 0 ? parts.join(' + ') : 'Free';
			return method === '*'
				? `${amount} on every method`
				: `${amount} on ${shortMethodLabel(methodLabels[method] ?? method)}`;
		})
		.join(', ');
}
