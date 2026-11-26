// Helpers for the admin-flow specs (login, products, providers, payments, cash-ins, webhooks,
// playground, live-mode). Kept apart from api.ts so the shared client stays untouched: everything
// here talks to the same endpoints through an APIRequestContext.
import { createHash, createHmac } from 'node:crypto';
import { createServer, type IncomingHttpHeaders, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import type { APIRequestContext, Page } from '@playwright/test';
import { ADMIN_TOKEN, MIDTRANS_SERVER_KEY, TOKEN_KEY } from './api';

/** The bullet used by masked secrets (`••••0001`). */
export const MASK = '\u2022\u2022\u2022\u2022';

/** Grouped id-ID digits, e.g. 150000 -> "150.000". */
export function grouped(n: number): string {
	return String(n).replace(/\B(?=(\d{3})+(?!\d))/g, '.');
}

/** Matches a rendered amount such as "Rp 150.000" (the UI separates with a non-breaking space). */
export function rp(n: number): RegExp {
	return new RegExp(`Rp\\s?${grouped(n).replace(/\./g, '\\.')}(?!\\d)`);
}

/** Mock/percent fee the server computes: round(amount * percent / 100) plus the fixed part. */
export function percentFee(amount: number, percent: number, fixed = 0): number {
	return fixed + Math.round((amount * percent) / 100);
}

export async function grantClipboard(page: Page): Promise<void> {
	await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
}

export function readClipboard(page: Page): Promise<string> {
	return page.evaluate(() => navigator.clipboard.readText());
}

/** Plants a token in localStorage without navigating (used to simulate a revoked session). */
export async function setStoredToken(page: Page, token: string | null): Promise<void> {
	await page.evaluate(
		([key, value]) => {
			if (value === null) localStorage.removeItem(key);
			else localStorage.setItem(key, value);
		},
		[TOKEN_KEY, token] as const
	);
}

/** Switches the desktop top-bar Sandbox / Live control; Live goes through the confirmation dialog. */
export async function switchModeDesktop(page: Page, mode: 'sandbox' | 'live'): Promise<void> {
	await page
		.getByTestId('mode-toggle')
		.getByRole('radio', { name: mode === 'live' ? 'Live' : 'Sandbox', exact: true })
		.click();
	if (mode === 'live') await page.getByTestId('mode-confirm-live').click();
}

// ---- Admin and product API (raw, for arrangement and checks) -------------------------------

export interface PaymentDetailJson {
	payment: { id: string; order_id: string; status: string; amount: number; snap_url: string };
	events: { id: string; outcome: string; verified: boolean }[];
	deliveries: DeliveryJson[];
	cash_in: { gross_amount: number; fee_amount: number; net_amount: number } | null;
}

export interface DeliveryJson {
	id: string;
	product_id: string;
	payment_id: string;
	event_id: string;
	event_type: string;
	status: 'pending' | 'delivered' | 'failed';
	attempts: number;
	last_status_code: number | null;
	last_error: string | null;
}

async function adminJson<T>(
	req: APIRequestContext,
	method: 'GET' | 'PATCH' | 'POST',
	path: string,
	data?: unknown
): Promise<T> {
	const res = await req.fetch(path, {
		method,
		headers: { Authorization: `Bearer ${ADMIN_TOKEN}`, 'content-type': 'application/json' },
		data: data === undefined ? undefined : JSON.stringify(data)
	});
	if (!res.ok()) throw new Error(`${method} ${path} -> ${res.status()} ${await res.text()}`);
	return (await res.json()) as T;
}

export function getPaymentDetail(req: APIRequestContext, id: string): Promise<PaymentDetailJson> {
	return adminJson(req, 'GET', `/admin/payments/${id}`);
}

export async function listDeliveries(
	req: APIRequestContext,
	productId: string
): Promise<DeliveryJson[]> {
	const page = await adminJson<{ data: DeliveryJson[] }>(
		req,
		'GET',
		`/admin/deliveries?product_id=${productId}&limit=50`
	);
	return page.data;
}

/** Calls the product API with a product key; returns the status and parsed body. */
export async function productCall(
	req: APIRequestContext,
	key: string,
	method: 'GET' | 'POST',
	path: string,
	data?: unknown
): Promise<{ status: number; body: unknown }> {
	const res = await req.fetch(path, {
		method,
		headers: { Authorization: `Bearer ${key}`, 'content-type': 'application/json' },
		data: data === undefined ? undefined : JSON.stringify(data)
	});
	const text = await res.text();
	let body: unknown = text;
	try {
		body = JSON.parse(text);
	} catch {
		// non-JSON body: keep the text
	}
	return { status: res.status(), body };
}

/** Midtrans notification signature: SHA512(order_id + status_code + gross_amount + server_key). */
export function midtransSignature(orderId: string, gross: string, serverKey = MIDTRANS_SERVER_KEY) {
	return createHash('sha512').update(`${orderId}200${gross}${serverKey}`).digest('hex');
}

/**
 * Posts a Midtrans notification to the inbound URL of `accountId`. `signature` overrides the
 * correct one (use a wrong value to simulate a forged request). Returns the HTTP status.
 */
export async function postMidtransNotification(
	req: APIRequestContext,
	accountId: string,
	orderId: string,
	amount: number,
	options: { status?: string; signature?: string; transactionId?: string } = {}
): Promise<number> {
	const gross = `${amount}.00`;
	const res = await req.post(`/hooks/${accountId}`, {
		data: {
			order_id: orderId,
			status_code: '200',
			gross_amount: gross,
			transaction_status: options.status ?? 'settlement',
			transaction_id: options.transactionId ?? `tx-${orderId.slice(0, 8)}`,
			payment_type: 'bank_transfer',
			va_numbers: [{ bank: 'bca', va_number: '123' }],
			signature_key: options.signature ?? midtransSignature(orderId, gross)
		}
	});
	return res.status();
}

// ---- Outbound webhook receiver ------------------------------------------------------------

export interface ReceivedRequest {
	method: string | undefined;
	url: string | undefined;
	headers: IncomingHttpHeaders;
	/** Raw body exactly as received, the bytes the signature covers. */
	body: string;
}

export interface Receiver {
	/** Full URL to register as a product webhook_url. */
	url: string;
	requests: ReceivedRequest[];
	/** HTTP status returned to the next and later deliveries. */
	setStatus(status: number): void;
	close(): Promise<void>;
}

/** A tiny local HTTP receiver on a random loopback port. Records every delivery. */
export async function startReceiver(initialStatus = 200): Promise<Receiver> {
	const requests: ReceivedRequest[] = [];
	let status = initialStatus;
	const server: Server = createServer((req, res) => {
		const chunks: Buffer[] = [];
		req.on('data', (chunk: Buffer) => chunks.push(chunk));
		req.on('end', () => {
			requests.push({
				method: req.method,
				url: req.url,
				headers: req.headers,
				body: Buffer.concat(chunks).toString('utf8')
			});
			res.writeHead(status, { 'content-type': 'text/plain' });
			res.end(status < 300 ? 'ok' : 'rejected');
		});
	});
	const listening = Promise.withResolvers<void>();
	server.listen(0, '127.0.0.1', () => listening.resolve());
	await listening.promise;
	const port = (server.address() as AddressInfo).port;
	return {
		url: `http://127.0.0.1:${port}/payrouters/webhook`,
		requests,
		setStatus: (next) => {
			status = next;
		},
		close: () => {
			const closed = Promise.withResolvers<void>();
			server.close(() => closed.resolve());
			return closed.promise;
		}
	};
}

/** Checks `PayRouters-Signature: t=<unix>,v1=<hex>` over `<t>.<raw body>` with the secret. */
export function verifyPayRoutersSignature(
	secret: string,
	header: string | undefined,
	rawBody: string
): boolean {
	if (!header) return false;
	const parts = Object.fromEntries(
		header.split(',').map((kv) => kv.split('=') as [string, string])
	);
	if (!parts.t || !parts.v1) return false;
	const expected = createHmac('sha256', secret).update(`${parts.t}.${rawBody}`).digest('hex');
	return expected === parts.v1;
}
