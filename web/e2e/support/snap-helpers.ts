// Helpers for the customer Snap page specs: payment creation through the product API, route stubs
// for states the mock provider cannot produce, and rupiah matchers that accept the id-ID no-break space.
import type { Page } from '@playwright/test';
import { type Api, type PaymentJson, uid } from './api';

/** Regex for `Rp 150.000` as the UI prints it (the space after Rp may be a no-break space). */
export function idr(amount: number): RegExp {
	const grouped = String(amount)
		.replace(/\B(?=(\d{3})+(?!\d))/g, '.')
		.replace(/\./g, '\\.');
	return new RegExp(`Rp\\s${grouped}`);
}

export const snapPath = (token: string): string => `/snap/${token}`;

export interface SnapPaymentRef {
	id: string;
	orderId: string;
	token: string;
	json: PaymentJson;
}

/** A mock (sandbox) payment created through the product API. Without `method` the customer picks on the page. */
export async function newSnapPayment(
	api: Api,
	key: string,
	body: Record<string, unknown> = {}
): Promise<SnapPaymentRef> {
	const orderId = (body.order_id as string | undefined) ?? uid('INV');
	const created = await api.createPayment(key, {
		amount: 150000,
		provider: 'mock',
		...body,
		order_id: orderId
	});
	if (created.status !== 201) {
		throw new Error(`create payment -> ${created.status} ${JSON.stringify(created.json)}`);
	}
	return {
		id: created.json.id,
		orderId,
		token: created.json.snap_url.split('/').pop() ?? '',
		json: created.json
	};
}

/** The fields of GET /snap/api/:token that the specs edit. Other fields pass through untouched. */
export interface SnapStateJson {
	merchant_name: string;
	payment: {
		status: string;
		provider: string;
		payable_amount: number | null;
		amount: number;
		instructions: Record<string, unknown> | null;
		[key: string]: unknown;
	};
	methods: unknown[];
	allow_simulate: boolean;
}

/** Serves the real GET state with `patch` applied, for payload shapes the mock provider cannot produce. */
export async function patchSnapState(
	page: Page,
	token: string,
	patch: (state: SnapStateJson) => void
): Promise<void> {
	await page.route(`**/snap/api/${token}`, async (route) => {
		const response = await route.fetch();
		const state = (await response.json()) as SnapStateJson;
		patch(state);
		await route.fulfill({
			status: response.status(),
			contentType: 'application/json',
			json: state
		});
	});
}

export const readClipboard = (page: Page): Promise<string> =>
	page.evaluate(() => navigator.clipboard.readText());
