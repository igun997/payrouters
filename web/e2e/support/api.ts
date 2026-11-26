import { createHash, randomBytes } from 'node:crypto';
import type { APIRequestContext } from '@playwright/test';

export const ADMIN_TOKEN = 'e2e-admin-token-0123456789abcdef';
export const TOKEN_KEY = 'payrouters.admin_token';
export const MIDTRANS_SERVER_KEY = 'SB-Mid-server-e2e-key-0001';
export const XENDIT_CALLBACK_TOKEN = 'e2e-xendit-callback-token';

export const uid = (prefix = 'e2e') => `${prefix}-${randomBytes(4).toString('hex')}`;

export interface PaymentJson {
	id: string;
	order_id: string;
	status: string;
	snap_url: string;
	amount: number;
	method: string | null;
	[key: string]: unknown;
}

export interface CreatedPayment {
	status: number;
	headers: Record<string, string>;
	json: PaymentJson;
}

export interface Product {
	id: string;
	name: string;
	webhookSecret: string;
	sandboxKey: string;
	liveKey: string;
}

/** Thin typed wrapper over the admin and product APIs. Fixtures use it to arrange state fast; the UI is what specs assert. */
export class Api {
	constructor(private readonly req: APIRequestContext) {}

	private async admin<T>(method: string, path: string, data?: unknown): Promise<T> {
		const res = await this.req.fetch(path, {
			method,
			headers: { Authorization: `Bearer ${ADMIN_TOKEN}`, 'content-type': 'application/json' },
			data: data === undefined ? undefined : JSON.stringify(data)
		});
		if (!res.ok()) throw new Error(`${method} ${path} -> ${res.status()} ${await res.text()}`);
		return res.status() === 204 ? (undefined as T) : ((await res.json()) as T);
	}

	async createProduct(name = uid('Shop'), webhookUrl?: string): Promise<Product> {
		const p = await this.admin<{ id: string; webhook_secret: string }>('POST', '/admin/products', {
			name,
			webhook_url: webhookUrl
		});
		const sandbox = await this.admin<{ key: string }>('POST', `/admin/products/${p.id}/api-keys`, {
			mode: 'sandbox',
			name: 'e2e'
		});
		const live = await this.admin<{ key: string }>('POST', `/admin/products/${p.id}/api-keys`, {
			mode: 'live',
			name: 'e2e'
		});
		return {
			id: p.id,
			name,
			webhookSecret: p.webhook_secret,
			sandboxKey: sandbox.key,
			liveKey: live.key
		};
	}

	async ensureProvider(
		provider: 'midtrans' | 'xendit',
		mode = 'sandbox'
	): Promise<{ id: string; webhookUrl: string }> {
		const existing = await this.admin<{
			data: { id: string; provider: string; mode: string; webhook_url: string }[];
		}>('GET', '/admin/providers');
		const found = existing.data.find((a) => a.provider === provider && a.mode === mode);
		if (found) return { id: found.id, webhookUrl: found.webhook_url };
		const credentials =
			provider === 'midtrans'
				? { server_key: MIDTRANS_SERVER_KEY }
				: { secret_key: 'xnd_development_e2e', callback_token: XENDIT_CALLBACK_TOKEN };
		const a = await this.admin<{ id: string; webhook_url: string }>('POST', '/admin/providers', {
			provider,
			mode,
			credentials
		});
		return { id: a.id, webhookUrl: a.webhook_url };
	}

	async setMockFees(fees: Record<string, { fixed: number; percent: number }>): Promise<void> {
		const list = await this.admin<{ data: { id: string; provider: string }[] }>(
			'GET',
			'/admin/providers'
		);
		const mock = list.data.find((a) => a.provider === 'mock')!;
		await this.admin('PATCH', `/admin/providers/${mock.id}`, { fees });
	}

	/** Product API call (Bearer product key). */
	async createPayment(key: string, body: Record<string, unknown>): Promise<CreatedPayment> {
		const res = await this.req.post('/v1/payments', {
			headers: { Authorization: `Bearer ${key}` },
			data: body
		});
		return {
			status: res.status(),
			headers: res.headers(),
			json: (await res.json()) as PaymentJson
		};
	}

	async simulate(token: string, outcome: 'paid' | 'expired' | 'failed') {
		const res = await this.req.post(`/snap/api/${token}/simulate`, { data: { outcome } });
		return res.status();
	}

	/** A paid mock payment ready for stats and recap specs. */
	async paidPayment(key: string, amount: number, method = 'va_bca', orderId = uid('ORD')) {
		const p = await this.createPayment(key, {
			order_id: orderId,
			amount,
			provider: 'mock',
			method
		});
		const token = p.json.snap_url.split('/').pop()!;
		await this.simulate(token, 'paid');
		return { orderId, id: p.json.id, token };
	}

	async midtransNotification(
		account: { id: string },
		paymentId: string,
		amount: number,
		status = 'settlement'
	) {
		const gross = `${amount}.00`;
		const signature = createHash('sha512')
			.update(`${paymentId}200${gross}${MIDTRANS_SERVER_KEY}`)
			.digest('hex');
		const res = await this.req.post(`/hooks/${account.id}`, {
			data: {
				order_id: paymentId,
				status_code: '200',
				gross_amount: gross,
				transaction_status: status,
				transaction_id: uid('tx'),
				payment_type: 'bank_transfer',
				va_numbers: [{ bank: 'bca', va_number: '123' }],
				signature_key: signature
			}
		});
		return res.status();
	}
}
