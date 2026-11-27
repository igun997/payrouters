// Long-content stress: extreme data on every dashboard page, dialog and Snap state, at six widths.
// Seeds 255-character names, a 64-character order ID with no spaces, a 200-character unbroken
// description word, 24-digit virtual accounts, Rp 9.999.999.999 payables, 80-character provider and
// OAuth client names, and long webhook errors and API key names. The layout detector in
// support/layout.ts reports every box past the viewport, every text overflow that is not exposed in
// full, and every overlap of two text boxes. Each width is one test on one page (sequential).
import { execFileSync } from 'node:child_process';
import { randomBytes, randomInt } from 'node:crypto';
import type { APIRequestContext, Page } from '@playwright/test';
import { ADMIN_TOKEN, Api, type Product, uid } from './support/api';
import { expect, randomClientIp, test } from './support/fixtures';
import { layoutIssues } from './support/layout';
import { BASE_URL, signIn } from './support/layout-helpers';
import { newAuthorization, registerClient } from './support/oauth';

const WIDTHS = [
	{ width: 320, height: 568 },
	{ width: 360, height: 640 },
	{ width: 390, height: 844 },
	{ width: 768, height: 1024 },
	{ width: 1024, height: 768 },
	{ width: 1366, height: 768 }
] as const;

const PORT = process.env.E2E_PORT ?? '8091';
const DB_URL = `postgres://payrouters:payrouters@127.0.0.1:5435/payrouters_e2e_${PORT}`;

const digits = (n: number): string => Array.from({ length: n }, () => randomInt(10)).join('');
const pad = (text: string, length: number): string =>
	text.repeat(Math.ceil(length / text.length)).slice(0, length);
const quote = (text: string): string => text.replaceAll("'", "''");

/** Runs SQL against the e2e database: provider-side fields the mock provider cannot produce. */
function sql(text: string): void {
	execFileSync('psql', [DB_URL, '-v', 'ON_ERROR_STOP=1', '-q', '-f', '-'], { input: text });
}

/** Overwrites what the customer sees to pay (VA, QRIS or Mandiri bill). Other fields are untouched. */
function setInstructions(
	paymentId: string,
	instructions: Record<string, unknown>,
	payable?: number
): void {
	const payableSet = payable === undefined ? '' : `, payable_amount = ${payable}`;
	sql(
		`UPDATE payments SET instructions = '${quote(JSON.stringify(instructions))}'::jsonb${payableSet} WHERE id = '${paymentId}';`
	);
}

const LONG_CUSTOMER = pad('Mohammad Kartika Dewi Anggraini Setiawan Putra Wijaya ', 255).trim();
const LONG_EMAIL = `${pad('very.long.address.', 90)}customer@payrouters-long-domain-example.test`;
const UNBROKEN = 'Z'.repeat(200);
const LONG_DESCRIPTION =
	`${UNBROKEN} ${pad('Pesanan paket tahunan dengan layanan tambahan ', 60)}`.slice(0, 255);
const LONG_ORDER_ID = randomBytes(32).toString('hex'); // 64 characters, no spaces
// A run token keeps this run's product distinct from products left by earlier runs (the list shows all).
const LONG_PRODUCT = `${randomBytes(3).toString('hex')} ${pad(
	'Perusahaan Teknologi Nusantara Sejahtera Platform Pembayaran Terintegrasi ',
	70
).trim()}`;
const LONG_WEBHOOK = `https://hooks.payrouters-example.test/${pad('segment-', 360)}webhook`;
const LONG_KEY_NAME = pad('Warehouse synchronisation service backend key ', 80).trim();
const LONG_PROVIDER_LABEL = pad(
	'PT Payrouters Nusantara Sejahtera International Trading and Distribution ',
	80
).trim();
const LONG_CLIENT_NAME = pad(
	'Assistant Integrasi Nusantara Sejahtera Enterprise Connector ',
	80
).trim();
const LONG_ERROR = `Status lookup timed out after 10 s while calling the provider ${pad('retry-trace-', 240)}`;
const VA_ACCOUNT_NAME = pad(
	'PT PAYROUTERS NUSANTARA SEJAHTERA INTERNATIONAL TRADING AND DISTRIBUTION ',
	80
).trim();

/** A 12-item cart whose prices add up to the payment amount (the API checks the sum). */
const CART_PRICE = 833_333_333;
const CART_AMOUNT = CART_PRICE * 12;
const CART_ITEMS = Array.from({ length: 12 }, (_, i) => ({
	name: `${i + 1}. ${pad('Paket layanan pembayaran lengkap dengan dukungan prioritas dan pelatihan ', 96).trim()}`,
	price: CART_PRICE,
	quantity: 1
}));

const LONG_METADATA = Object.fromEntries(
	Array.from({ length: 20 }, (_, i) => [
		`metadata_key_${i}_${pad('k', 44)}`,
		`value ${i} ${pad('v', 180)}`
	])
);

const LONG_QRIS = `00020101021226${pad('A1B2C3D4E5F6', 480)}5303360`;

/** Ids and tokens the visits navigate to, filled in by the seed. */
interface Seed {
	product: Product;
	longKey: string;
	providerId: string;
	originalLabel: string;
	va: string;
	paid: string;
	qris: string;
	mandiri: string;
	vaToken: string;
	paidToken: string;
	qrisToken: string;
	mandiriToken: string;
	choose: string;
	expired: string;
	failed: string;
	missing: string;
	consentPath: string;
}

interface Visit {
	name: string;
	path: (seed: Seed) => string;
	/** Selector that must be visible on the page before any step runs. */
	ready: string;
	/** Optional interaction after the page is ready (opens a dialog or switches a tab). */
	steps?: (page: Page, seed: Seed) => Promise<void>;
	/** Selector that must be visible once `steps` has run (the dialog or tab content it opened). */
	after?: string;
	scheme?: 'light' | 'dark';
}

/** The long product's row in the product list (the list also holds other products). */
const productRow = (page: Page, name: string) =>
	page.getByTestId('product-row').filter({ hasText: name }).first();

/** The keys panel toggles from whichever expand button this width shows (the labelled one is hidden below 360 and from lg). */
const openKeys = async (page: Page, name: string): Promise<void> => {
	// Already open on wide screens: only press a toggle that is closed.
	const closed = productRow(page, name)
		.locator('button[aria-expanded="false"]:visible')
		.filter({ hasText: /API keys/ })
		.first();
	if ((await closed.count()) > 0) await closed.dispatchEvent('click');
};

async function seedAll(req: APIRequestContext, api: Api): Promise<Seed> {
	const product = await api.createProduct(LONG_PRODUCT, LONG_WEBHOOK);
	const keyRes = await req.fetch(`/admin/products/${product.id}/api-keys`, {
		method: 'POST',
		headers: { Authorization: `Bearer ${ADMIN_TOKEN}`, 'content-type': 'application/json' },
		data: JSON.stringify({ mode: 'sandbox', name: LONG_KEY_NAME })
	});
	expect(keyRes.status()).toBe(201);
	const longKeyBody = (await keyRes.json()) as { key: string };

	const midtrans = await api.ensureProvider('midtrans');
	const providersRes = await req.fetch('/admin/providers', {
		headers: { Authorization: `Bearer ${ADMIN_TOKEN}` }
	});
	const providerList = (await providersRes.json()) as { data: { id: string; label: string }[] };
	const original = providerList.data.find((a) => a.id === midtrans.id);
	const originalLabel = original?.label ?? 'Midtrans sandbox';
	await req.fetch(`/admin/providers/${midtrans.id}`, {
		method: 'PATCH',
		headers: { Authorization: `Bearer ${ADMIN_TOKEN}`, 'content-type': 'application/json' },
		data: JSON.stringify({ label: LONG_PROVIDER_LABEL })
	});

	const key = longKeyBody.key;
	const base = {
		amount: 9_999_999_999,
		provider: 'mock',
		description: LONG_DESCRIPTION,
		customer: { name: LONG_CUSTOMER, email: LONG_EMAIL },
		metadata: LONG_METADATA,
		return_url: `https://merchant.payrouters-example.test/return/${pad('segment-', 440)}`
	};
	const vaBody = { method: 'va_bca', order_id: LONG_ORDER_ID };
	const virtualAccount = await api.createPayment(key, { ...base, ...vaBody });
	expect(virtualAccount.status).toBe(201);
	setInstructions(
		virtualAccount.json.id,
		{
			type: 'virtual_account',
			bank: 'bca',
			account_number: digits(24),
			account_name: VA_ACCOUNT_NAME,
			biller_code: null,
			bill_key: null
		},
		9_999_999_999
	);

	const paidVa = await api.createPayment(key, {
		...base,
		method: 'va_bca',
		order_id: randomBytes(32).toString('hex')
	});
	setInstructions(
		paidVa.json.id,
		{
			type: 'virtual_account',
			bank: 'bca',
			account_number: digits(24),
			account_name: VA_ACCOUNT_NAME,
			biller_code: null,
			bill_key: null
		},
		9_999_999_999
	);
	expect(await api.simulate(paidVa.json.snap_url.split('/').pop() ?? '', 'paid')).toBe(200);

	const qris = await api.createPayment(key, {
		...base,
		method: 'qris',
		order_id: randomBytes(32).toString('hex')
	});
	setInstructions(
		qris.json.id,
		{ type: 'qris', qr_string: LONG_QRIS, qr_image_url: null },
		9_999_999_999
	);

	const mandiri = await api.createPayment(key, {
		...base,
		method: 'va_mandiri',
		order_id: randomBytes(32).toString('hex')
	});
	setInstructions(
		mandiri.json.id,
		{
			type: 'virtual_account',
			bank: 'mandiri',
			account_number: null,
			account_name: null,
			biller_code: '70012',
			bill_key: digits(24)
		},
		9_999_999_999
	);

	const choose = await api.createPayment(key, {
		provider: 'mock',
		order_id: randomBytes(32).toString('hex'),
		amount: CART_AMOUNT,
		description: LONG_DESCRIPTION,
		customer: { name: LONG_CUSTOMER, email: LONG_EMAIL },
		items: CART_ITEMS,
		metadata: LONG_METADATA
	});
	expect(choose.status).toBe(201);

	const expired = await api.createPayment(key, {
		...base,
		method: 'va_bca',
		order_id: randomBytes(32).toString('hex')
	});
	expect(await api.simulate(expired.json.snap_url.split('/').pop() ?? '', 'expired')).toBe(200);
	const failed = await api.createPayment(key, {
		...base,
		method: 'va_bca',
		order_id: randomBytes(32).toString('hex')
	});
	expect(await api.simulate(failed.json.snap_url.split('/').pop() ?? '', 'failed')).toBe(200);

	// The paid payment's webhook delivery gave up with a long error; one inbound event has a long error too.
	sql(`
UPDATE webhook_deliveries SET status = 'failed', attempts = 8, delivered_at = NULL, next_attempt_at = NULL,
  last_status_code = NULL, last_error = '${quote(LONG_ERROR)}' WHERE payment_id = '${paidVa.json.id}';
INSERT INTO provider_events (id, provider_account_id, provider, payment_id, verified, outcome, error, headers, body_text, received_at)
VALUES (gen_random_uuid(), '${midtrans.id}', 'midtrans', '${paidVa.json.id}', true, 'error', '${quote(LONG_ERROR)}', '{}'::jsonb,
  '{"transaction_status":"pending","order_id":"${LONG_ORDER_ID}"}', now() - interval '3 minutes');
`);

	// One approved OAuth connection (for the integrations list) and a pending consent for a long client name.
	const client = await registerClient(req, LONG_CLIENT_NAME);
	const approved = await newAuthorization(req, BASE_URL, { client, scope: 'read write' });
	const approvedParams = new URL(approved.path, BASE_URL).searchParams;
	const approval = await req.fetch('/admin/oauth/authorize', {
		method: 'POST',
		headers: { Authorization: `Bearer ${ADMIN_TOKEN}`, 'content-type': 'application/json' },
		data: JSON.stringify({ ...Object.fromEntries(approvedParams), decision: 'allow' })
	});
	const approvalBody = (await approval.json()) as { redirect_url: string };
	const code = new URL(approvalBody.redirect_url).searchParams.get('code');
	await req.post('/oauth/token', {
		form: {
			grant_type: 'authorization_code',
			code: code ?? '',
			redirect_uri: approved.client.redirect_uris[0],
			client_id: client.client_id,
			code_verifier: approved.verifier
		}
	});
	const consent = await newAuthorization(req, BASE_URL, { client, scope: 'read write' });

	const token = (created: { json: { snap_url: string } }) =>
		created.json.snap_url.split('/').pop() ?? '';
	return {
		product,
		longKey: key,
		providerId: midtrans.id,
		originalLabel,
		va: virtualAccount.json.id,
		paid: paidVa.json.id,
		qris: qris.json.id,
		mandiri: mandiri.json.id,
		vaToken: token(virtualAccount),
		paidToken: token(paidVa),
		qrisToken: token(qris),
		mandiriToken: token(mandiri),
		choose: token(choose),
		expired: token(expired),
		failed: token(failed),
		missing: uid('missing'),
		consentPath: consent.path
	};
}

const VISITS: Visit[] = [
	{ name: 'Overview', path: () => '/', ready: '[data-testid="kpi-conversion"]' },
	{
		name: 'Overview dark',
		path: () => '/',
		ready: '[data-testid="kpi-conversion"]',
		scheme: 'dark'
	},
	{ name: 'Payments', path: () => '/payments', ready: '[data-testid="payment-row"]' },
	{
		name: 'Payments filter sheet',
		path: () => '/payments',
		ready: '[data-testid="payment-row"]',
		steps: async (page) => {
			const open = page.getByRole('button', { name: 'Open filters' });
			if (await open.isVisible()) await open.click();
			await page.waitForTimeout(400);
		}
	},
	{
		name: 'Payment detail (pending VA)',
		path: (s) => `/payments/${s.va}`,
		ready: '[data-testid="payment-order-id"]'
	},
	{
		name: 'Payment detail dark (pending VA)',
		path: (s) => `/payments/${s.va}`,
		ready: '[data-testid="payment-order-id"]',
		scheme: 'dark'
	},
	{
		name: 'Payment detail (paid VA)',
		path: (s) => `/payments/${s.paid}`,
		ready: '[data-testid="payment-order-id"]'
	},
	{ name: 'Products', path: () => '/products', ready: '[data-testid="product-row"]' },
	{
		name: 'Products keys expanded',
		path: () => '/products',
		ready: '[data-testid="product-row"]',
		steps: async (page, s) => {
			await openKeys(page, s.product.name);
		},
		after: '[data-testid="api-key-row"]'
	},
	{
		name: 'Product edit dialog',
		path: () => '/products',
		ready: '[data-testid="product-row"]',
		steps: async (page, s) => {
			await page
				.getByRole('button', { name: `Edit ${s.product.name}`, exact: true })
				.dispatchEvent('click');
		},
		after: '[data-testid="dialog-edit-product"]'
	},
	{
		name: 'Create API key dialog',
		path: () => '/products',
		ready: '[data-testid="product-row"]',
		steps: async (page, s) => {
			await openKeys(page, s.product.name);
			await productRow(page, s.product.name)
				.getByTestId('create-api-key-button')
				.dispatchEvent('click');
		},
		after: '[data-testid="dialog-create-key"]'
	},
	{
		name: 'New product dialog',
		path: () => '/products',
		ready: '[data-testid="page-products"]',
		steps: async (page) => {
			await page.getByTestId('new-product-button').dispatchEvent('click');
		},
		after: '[data-testid="dialog-new-product"]'
	},
	{ name: 'Providers', path: () => '/providers', ready: '[data-testid="provider-row"]' },
	{
		name: 'Provider edit dialog',
		path: () => '/providers',
		ready: '[data-testid="provider-row"]',
		steps: async (page) => {
			await page
				.getByRole('button', { name: `Edit ${LONG_PROVIDER_LABEL}`, exact: true })
				.dispatchEvent('click');
		},
		after: '[data-testid="dialog-edit-provider"]'
	},
	{ name: 'Webhooks inbound', path: () => '/webhooks', ready: '[data-testid="webhook-event-row"]' },
	{
		name: 'Webhooks outbound',
		path: () => '/webhooks',
		ready: '[data-testid="webhook-event-row"]',
		steps: async (page) => {
			await page.getByRole('tab', { name: 'Outbound deliveries' }).click();
		},
		after: '[data-testid="delivery-row"]'
	},
	{ name: 'Cash-ins', path: () => '/cash-ins', ready: '[data-testid="cash-in-totals"]' },
	{
		name: 'Cash-ins filter sheet',
		path: () => '/cash-ins',
		ready: '[data-testid="cash-in-totals"]',
		steps: async (page) => {
			const open = page.getByTestId('open-cash-in-filters');
			if (await open.isVisible()) await open.click();
			await page.waitForTimeout(400);
		}
	},
	{ name: 'Playground', path: () => '/playground', ready: '[data-testid="playground-form"]' },
	{ name: 'Docs', path: () => '/docs', ready: '[data-testid="page-docs"]' },
	{ name: 'Integrations', path: () => '/integrations', ready: '[data-testid="connection-row"]' },
	{
		name: 'OAuth consent (80-char client)',
		path: (s) => s.consentPath,
		ready: '[data-testid="page-oauth-authorize"]'
	},
	{ name: 'Snap choose', path: (s) => `/snap/${s.choose}`, ready: '[data-testid="snap-methods"]' },
	{ name: 'Snap VA', path: (s) => `/snap/${s.vaToken}`, ready: '[data-testid="snap-va"]' },
	{ name: 'Snap QRIS', path: (s) => `/snap/${s.qrisToken}`, ready: '[data-testid="snap-qris"]' },
	{
		name: 'Snap Mandiri bill',
		path: (s) => `/snap/${s.mandiriToken}`,
		ready: '[data-testid="snap-mandiri-bill"]'
	},
	{
		name: 'Snap paid',
		path: (s) => `/snap/${s.paidToken}`,
		ready: '[data-testid="snap-status"][data-status="paid"]'
	},
	{
		name: 'Snap expired',
		path: (s) => `/snap/${s.expired}`,
		ready: '[data-testid="snap-status"][data-status="expired"]'
	},
	{
		name: 'Snap failed',
		path: (s) => `/snap/${s.failed}`,
		ready: '[data-testid="snap-status"][data-status="failed"]'
	},
	{
		name: 'Snap not found',
		path: (s) => `/snap/${s.missing}`,
		ready: '[data-testid="snap-status"][data-status="not_found"]'
	}
];

/** Visits every page in order at one width on one page; returns the defects, each prefixed with the width and page. */
async function sweep(page: Page, seed: Seed, width: (typeof WIDTHS)[number]): Promise<string[]> {
	// Bounded actions: a click that cannot land fails with its selector instead of stalling the run.
	page.setDefaultTimeout(15_000);
	await page.setViewportSize({ width: width.width, height: width.height });
	const failures: string[] = [];
	for (const visit of VISITS) {
		const label = `${width.width}px ${visit.name}`;
		try {
			// A same-origin page first, so the colour scheme can be set in its localStorage.
			await page.goto('/login');
			await page.evaluate(
				([k, v]) => localStorage.setItem(k, v),
				['mode-watcher-mode', visit.scheme ?? 'light']
			);
			await page.goto(visit.path(seed));
			await expect(page.locator(visit.ready).first()).toBeVisible();
			await page.evaluate(() => document.fonts.ready);
			if (visit.steps) await visit.steps(page, seed);
			if (visit.after) await expect(page.locator(visit.after).first()).toBeVisible();
			await page.waitForTimeout(150);
			for (const issue of await layoutIssues(page)) failures.push(`${label}: ${issue}`);
		} catch (error) {
			const shown = await page
				.locator('body')
				.innerText()
				.catch(() => '');
			const reason = (error as Error).message.split('\n')[0];
			failures.push(
				`${label}: visit failed: ${reason} | page shows: ${shown.replace(/\s+/g, ' ').slice(0, 200)}`
			);
		}
	}
	return failures;
}

test.describe('long content', () => {
	let seed: Seed;

	test.beforeAll(async ({ playwright }) => {
		const req = await playwright.request.newContext({
			baseURL: BASE_URL,
			extraHTTPHeaders: { 'X-Forwarded-For': randomClientIp() }
		});
		try {
			seed = await seedAll(req, new Api(req));
		} finally {
			await req.dispose();
		}
	});

	test.afterAll(async ({ playwright }) => {
		if (!seed) return;
		// Restore the shared midtrans account label that the seed made 80 characters long.
		const req = await playwright.request.newContext({
			baseURL: BASE_URL,
			extraHTTPHeaders: { 'X-Forwarded-For': randomClientIp() }
		});
		await req.fetch(`/admin/providers/${seed.providerId}`, {
			method: 'PATCH',
			headers: { Authorization: `Bearer ${ADMIN_TOKEN}`, 'content-type': 'application/json' },
			data: JSON.stringify({ label: seed.originalLabel })
		});
		await req.dispose();
	});

	// One test per width, run one after another on one page: each sweep stays well inside its timeout.
	for (const w of WIDTHS) {
		test(`no offscreen boxes, unexposed text overflow or overlapping text at ${w.width}px`, async ({
			page
		}) => {
			test.setTimeout(240_000);
			await signIn(page);
			const failures = await sweep(page, seed, w);
			expect(failures, failures.join('\n')).toEqual([]);
		});
	}
});
