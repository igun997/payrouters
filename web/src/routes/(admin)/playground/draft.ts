import type { AdminCreatePaymentBody, Method, Mode, Provider } from '#lib/api/types';
import { formatIdr } from '#lib/format';
import { safeHttpUrl } from '#lib/url';

/** The playground form. Money is kept as digits so the input can format it as it is typed. */
export type PaymentDraft = {
	productId: string;
	provider: Provider | '';
	checkout: 'snap' | 'provider';
	method: Method | '';
	/** Whole rupiah, digits only. */
	amount: string;
	orderId: string;
	description: string;
	customerName: string;
	customerEmail: string;
	customerPhone: string;
	/** Seconds, digits only; empty uses the server default (24 h). */
	expiresIn: string;
	returnUrl: string;
	idrxWallet: string;
	/** Chain id as text; empty uses the IDRX account default. */
	idrxChain: '' | '8453';
};

export type DraftField = keyof PaymentDraft;
export type DraftErrors = Partial<Record<DraftField, string>>;

export const MIN_AMOUNT = 1;
/** IDRX mints at least this many rupiah-equivalent units (api.md: IDRX amount min 20000). */
export const IDRX_MIN_AMOUNT = 20000;
export const MIN_EXPIRES_IN = 300;
export const ORDER_ID_MAX = 64;

const WALLET = /^0x[0-9a-fA-F]{40}$/;
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const PHONE = /^\+?\d{8,15}$/;

export function newOrderId(): string {
	return `PG-${crypto.randomUUID().slice(0, 8).toUpperCase()}`;
}

export function blankDraft(): PaymentDraft {
	return {
		productId: '',
		provider: '',
		checkout: 'snap',
		method: '',
		amount: '150000',
		orderId: newOrderId(),
		description: '',
		customerName: '',
		customerEmail: '',
		customerPhone: '',
		expiresIn: '',
		returnUrl: '',
		idrxWallet: '',
		idrxChain: ''
	};
}

/** Digits to a grouped display string, e.g. "150000" -> "150.000" (id-ID grouping). */
export function formatAmountInput(digits: string): string {
	return digits === '' ? '' : Number(digits).toLocaleString('id-ID');
}

export function amountValue(draft: PaymentDraft): number {
	return draft.amount === '' ? 0 : Number(draft.amount);
}

/** Client-side checks that mirror the API. Empty optional fields are not errors. */
export function validateDraft(draft: PaymentDraft): DraftErrors {
	const errors: DraftErrors = {};
	const amount = amountValue(draft);

	if (draft.productId === '') errors.productId = 'Choose a product';

	const orderId = draft.orderId.trim();
	if (orderId === '') errors.orderId = 'Enter an order ID';
	else if (orderId.length > ORDER_ID_MAX)
		errors.orderId = `Use ${ORDER_ID_MAX} characters or fewer`;

	if (amount < MIN_AMOUNT) errors.amount = `Enter an amount of at least ${formatIdr(MIN_AMOUNT)}`;
	else if (draft.provider === 'idrx' && amount < IDRX_MIN_AMOUNT)
		errors.amount = `IDRX mints at least ${formatIdr(IDRX_MIN_AMOUNT)}`;

	if (draft.customerEmail.trim() !== '' && !EMAIL.test(draft.customerEmail.trim()))
		errors.customerEmail = 'Enter a valid email address';

	if (draft.customerPhone.trim() !== '' && !PHONE.test(draft.customerPhone.replace(/[\s-]/g, '')))
		errors.customerPhone = 'Use digits with the country code, e.g. +628123456789';

	if (draft.expiresIn !== '' && Number(draft.expiresIn) < MIN_EXPIRES_IN)
		errors.expiresIn = `Enter at least ${MIN_EXPIRES_IN} seconds`;

	if (draft.returnUrl.trim() !== '' && safeHttpUrl(draft.returnUrl.trim()) === null)
		errors.returnUrl = 'Enter a full address starting with https://';

	if (
		draft.provider === 'idrx' &&
		draft.idrxWallet.trim() !== '' &&
		!WALLET.test(draft.idrxWallet.trim())
	)
		errors.idrxWallet = 'Enter a 0x wallet address, 42 characters';

	return errors;
}

const optional = (value: string): string | undefined => {
	const trimmed = value.trim();
	return trimmed === '' ? undefined : trimmed;
};

/** The POST /admin/payments body for a valid draft. */
export function buildPaymentBody(draft: PaymentDraft, mode: Mode): AdminCreatePaymentBody {
	const name = optional(draft.customerName);
	const email = optional(draft.customerEmail);
	const phone = optional(draft.customerPhone);
	const wallet = draft.provider === 'idrx' ? optional(draft.idrxWallet) : undefined;
	const chain =
		draft.provider === 'idrx' && draft.idrxChain !== '' ? Number(draft.idrxChain) : undefined;
	const expiresIn = optional(draft.expiresIn);

	return {
		product_id: draft.productId,
		mode,
		order_id: draft.orderId.trim(),
		amount: amountValue(draft),
		provider: draft.provider === '' ? undefined : draft.provider,
		checkout: draft.checkout,
		method: draft.checkout === 'snap' && draft.method !== '' ? draft.method : undefined,
		description: optional(draft.description),
		customer: name || email || phone ? { name, email, phone } : undefined,
		expires_in: expiresIn === undefined ? undefined : Number(expiresIn),
		return_url: optional(draft.returnUrl),
		provider_options:
			wallet !== undefined || chain !== undefined
				? { destination_wallet_address: wallet, chain_id: chain }
				: undefined
	};
}
