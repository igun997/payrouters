// Fills a fresh PayRouters database with demo data and captures the README screenshots.
//
//   cd web && pnpm build
//   E2E_PORT=8120 ./e2e/support/start-server.sh &        # fresh database payrouters_e2e_8120
//   node scripts/capture-screenshots.mjs http://127.0.0.1:8120 \
//     postgres://payrouters:payrouters@127.0.0.1:5435/payrouters_e2e_8120
//
// Payments are created through the real API (mock provider), then spread over 30 days and spread
// across Midtrans and Xendit accounts with SQL, so the charts and tables look like a live shop.
// Writes ../docs/screenshots/*.png. Demo data only: the credentials and webhook hosts are fake.
import { execFileSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';

const [, , BASE, DB_URL] = process.argv;
if (!BASE || !DB_URL) throw new Error('usage: capture-screenshots.mjs <base url> <database url>');
const ADMIN = process.env.ADMIN_TOKEN ?? 'e2e-admin-token-0123456789abcdef';
const OUT = join(dirname(fileURLToPath(import.meta.url)), '../../docs/screenshots');
mkdirSync(OUT, { recursive: true });

// Deterministic pseudo-random numbers, so a rerun gives the same pictures.
let state = 20261126;
const rnd = () => (state = (state * 1664525 + 1013904223) >>> 0) / 2 ** 32;
const pick = (list) => list[Math.floor(rnd() * list.length)];

async function api(method, path, body, token = ADMIN) {
	const res = await fetch(BASE + path, {
		method,
		headers: { Authorization: `Bearer ${token}`, 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!res.ok) throw new Error(`${method} ${path} ${res.status} ${await res.text()}`);
	return res.status === 204 ? undefined : res.json();
}
const sql = (text) =>
	execFileSync('psql', [DB_URL, '-v', 'ON_ERROR_STOP=1', '-q', '-f', '-'], { input: text });

// --- Providers and products -------------------------------------------------------------------
await api('POST', '/admin/providers', {
	provider: 'midtrans',
	mode: 'sandbox',
	credentials: { server_key: 'SB-Mid-server-demo-Kq7w1dFa' },
	fees: {
		va_bca: { fixed: 4000, percent: 0 },
		va_bni: { fixed: 4000, percent: 0 },
		qris: { fixed: 0, percent: 0.7 }
	}
}).catch(() => {});
await api('POST', '/admin/providers', {
	provider: 'xendit',
	mode: 'sandbox',
	credentials: {
		secret_key: 'xnd_development_demoSecretKey9f3c',
		callback_token: 'demo-callback-token-2d1e'
	},
	fees: { '*': { fixed: 4500, percent: 0 } }
}).catch(() => {});
const accounts = (await api('GET', '/admin/providers')).data;
const account = (p) => accounts.find((a) => a.provider === p && a.mode === 'sandbox');

const productDefs = [
	{ name: 'Demo Shop', webhook_url: 'https://api.demoshop.id/payrouters/webhook' },
	{ name: 'Kelas Coding', webhook_url: 'https://kelas.example.id/hooks/payrouters' },
	{ name: 'Ledgerly', webhook_url: 'https://app.ledgerly.id/payrouters/webhook' }
];
const products = [];
for (const def of productDefs) {
	const p = await api('POST', '/admin/products', def);
	const key = await api('POST', `/admin/products/${p.id}/api-keys`, {
		mode: 'sandbox',
		name: 'backend'
	});
	products.push({ ...p, key: key.key });
}

// --- Payments ---------------------------------------------------------------------------------
const methods = ['va_bca', 'va_bni', 'va_bri', 'va_mandiri', 'qris'];
const amounts = [
	49000, 75000, 89000, 99000, 129000, 149000, 199000, 249000, 349000, 499000, 750000
];
const created = [];
for (let i = 0; i < 72; i++) {
	const product = pick(products);
	const method = pick(methods);
	const p = await api(
		'POST',
		'/v1/payments',
		{
			order_id: `INV-${20300 + i}`,
			amount: pick(amounts),
			provider: 'mock',
			method,
			customer: {
				name: pick(['Budi Santoso', 'Siti Rahma', 'Andi Pratama', 'Dewi Lestari']),
				email: 'customer@example.com'
			},
			description: `Order #${20300 + i}`
		},
		product.key
	);
	const roll = rnd();
	const outcome =
		roll < 0.8 ? 'paid' : roll < 0.88 ? 'pending' : roll < 0.95 ? 'expired' : 'failed';
	if (outcome !== 'pending') {
		await api('POST', `/snap/api/${p.snap_url.split('/').pop()}/simulate`, { outcome }, '');
	}
	created.push({ id: p.id, orderId: p.order_id, amount: p.amount, outcome });
}

// One open payment on a VA for the customer page screenshot, and one waiting for a method.
const open = await api(
	'POST',
	'/v1/payments',
	{
		order_id: 'INV-20999',
		amount: 249000,
		provider: 'mock',
		method: 'va_bca',
		description: 'Pro plan (1 month)',
		customer: { name: 'Budi Santoso' }
	},
	products[0].key
);
const choose = await api(
	'POST',
	'/v1/payments',
	{ order_id: 'INV-21000', amount: 149000, provider: 'mock', description: 'Pro plan (1 month)' },
	products[0].key
);
const openToken = open.snap_url.split('/').pop();
const chooseToken = choose.snap_url.split('/').pop();

// --- Spread over 30 days, split across providers ----------------------------------------------
const assigned = new Map();
const rows = created.map((p, i) => {
	// Newer payments are more common; a few hours back for the most recent ones.
	const hoursAgo =
		p.outcome === 'pending' ? 1 + Math.floor(rnd() * 18) : Math.floor(rnd() ** 1.6 * 29 * 24) + 2;
	const provider = rnd() < 0.55 ? 'midtrans' : rnd() < 0.8 ? 'xendit' : 'mock';
	assigned.set(p.id, provider);
	return `('${p.id}', ${hoursAgo}, ${(i % 7) + 3}, '${provider}', '${account(provider).id}')`;
});
sql(`
UPDATE payments p SET
  created_at = now() - v.h * interval '1 hour',
  updated_at = now() - v.h * interval '1 hour' + v.m * interval '1 minute',
  expires_at = CASE WHEN p.status = 'pending' THEN now() + interval '5 hours'
                    ELSE now() - v.h * interval '1 hour' + interval '24 hours' END,
  paid_at = CASE WHEN p.status = 'paid' THEN now() - v.h * interval '1 hour' + v.m * interval '1 minute' END,
  provider = v.provider,
  provider_account_id = v.account::uuid
FROM (VALUES ${rows.join(',')}) AS v(id, h, m, provider, account)
WHERE p.id = v.id::uuid;
UPDATE cash_ins c SET paid_at = p.paid_at, provider = p.provider, provider_account_id = p.provider_account_id
FROM payments p WHERE c.payment_id = p.id;
-- Provider fees: Midtrans VA 4.000 and QRIS 0,7%; Xendit 4.500; mock 0,7%.
UPDATE cash_ins c SET fee_amount = x.fee, net_amount = c.gross_amount - x.fee
FROM (SELECT id, CASE provider
        WHEN 'midtrans' THEN CASE WHEN method = 'qris' THEN round(gross_amount * 0.007) ELSE 4000 END
        WHEN 'xendit' THEN 4500 ELSE round(gross_amount * 0.007) END AS fee FROM cash_ins) x
WHERE c.id = x.id;
UPDATE payments p SET fee_amount = c.fee_amount, net_amount = c.net_amount FROM cash_ins c WHERE c.payment_id = p.id;
-- The simulator's own inbox rows would crowd out the provider events inserted below.
DELETE FROM provider_events WHERE provider = 'mock';
UPDATE webhook_deliveries d SET created_at = COALESCE(p.paid_at, p.created_at) + interval '1 second'
FROM payments p WHERE d.payment_id = p.id;
-- Most deliveries succeeded; a few are still retrying or gave up.
UPDATE webhook_deliveries SET status = 'delivered', attempts = 1, next_attempt_at = NULL,
  last_status_code = 200, last_error = NULL, delivered_at = created_at + interval '1 second';
WITH pick AS (SELECT id, row_number() OVER (ORDER BY created_at DESC) AS n FROM webhook_deliveries)
UPDATE webhook_deliveries d SET status = 'pending', attempts = 2, delivered_at = NULL,
  next_attempt_at = now() + interval '28 minutes', last_status_code = 503, last_error = 'HTTP 503'
FROM pick WHERE d.id = pick.id AND pick.n IN (2, 9);
WITH pick AS (SELECT id, row_number() OVER (ORDER BY created_at DESC) AS n FROM webhook_deliveries)
UPDATE webhook_deliveries d SET status = 'failed', attempts = 8, delivered_at = NULL,
  next_attempt_at = NULL, last_status_code = NULL, last_error = 'Timed out after 10 s'
FROM pick WHERE d.id = pick.id AND pick.n = 16;
`);

// --- Inbound webhook inbox ---------------------------------------------------------------------
const paidBy = (provider) =>
	created.filter((p) => p.outcome === 'paid' && assigned.get(p.id) === provider);
const mid = paidBy('midtrans');
const xen = paidBy('xendit');
const midBody = (p, type) => ({
	transaction_status: 'settlement',
	order_id: p.orderId,
	gross_amount: `${p.amount}.00`,
	payment_type: type,
	fraud_status: 'accept',
	signature_key: '[redacted]'
});
const event = (provider, outcome, verified, payment, error, body, minutesAgo) =>
	`(gen_random_uuid(), '${account(provider).id}', '${provider}', ${payment ? `'${payment.id}'` : 'NULL'}, ${verified}, '${outcome}', ${error ? `'${error}'` : 'NULL'}, '${JSON.stringify(
		provider === 'midtrans'
			? {
					'content-type': 'application/json',
					'user-agent': 'Midtrans/1.0',
					authorization: '[redacted]'
				}
			: { 'content-type': 'application/json', 'x-callback-token': '[redacted]' }
	)}'::jsonb, '${JSON.stringify(body).replaceAll("'", "''")}', now() - interval '${minutesAgo} minutes')`;
sql(`
INSERT INTO provider_events (id, provider_account_id, provider, payment_id, verified, outcome, error, headers, body_text, received_at) VALUES
${[
	event('midtrans', 'applied', true, mid[0], null, midBody(mid[0], 'bank_transfer'), 14),
	event(
		'xendit',
		'noop',
		true,
		xen[0],
		null,
		{
			event: 'payment.succeeded',
			data: { reference_id: xen[0].orderId, status: 'SUCCEEDED', amount: xen[0].amount }
		},
		52
	),
	event(
		'midtrans',
		'unmatched',
		true,
		null,
		'No payment for order MINT-0093',
		{
			transaction_status: 'settlement',
			order_id: 'MINT-0093',
			gross_amount: '2000000.00',
			signature_key: '[redacted]'
		},
		97
	),
	event(
		'xendit',
		'rejected',
		false,
		null,
		'x-callback-token mismatch',
		{ event: 'payment.succeeded', data: { reference_id: 'INV-20401', status: 'SUCCEEDED' } },
		181
	),
	event('midtrans', 'applied', true, mid[1], null, midBody(mid[1], 'qris'), 260),
	event(
		'midtrans',
		'error',
		true,
		mid[2],
		'Status lookup timed out after 10 s',
		{ ...midBody(mid[2], 'bank_transfer'), transaction_status: 'pending' },
		410
	)
].join(',\n')};
`);

// --- Screenshots --------------------------------------------------------------------------------
const browser = await chromium.launch({ channel: 'chromium' });
async function shot(name, { width, height, path, scale = 1, prepare }) {
	const context = await browser.newContext({
		viewport: { width, height },
		deviceScaleFactor: scale,
		timezoneId: 'Asia/Jakarta',
		locale: 'en-US'
	});
	await context.addInitScript((t) => localStorage.setItem('payrouters.admin_token', t), ADMIN);
	const page = await context.newPage();
	await page.goto(BASE + path);
	await page.waitForLoadState('networkidle');
	await page.waitForTimeout(900);
	if (prepare) await prepare(page);
	await page.waitForTimeout(500);
	await page.screenshot({ path: join(OUT, `${name}.png`) });
	console.log('wrote', name);
	await context.close();
}

const detail = { id: mid[0].id };

await shot('overview', { width: 1366, height: 800, path: '/' });
await shot('payments', { width: 1366, height: 800, path: '/payments' });
await shot('payment-detail', { width: 1366, height: 1000, path: `/payments/${detail.id}` });
await shot('cash-ins', { width: 1366, height: 800, path: '/cash-ins' });
await shot('webhooks', {
	width: 1366,
	height: 860,
	path: '/webhooks',
	prepare: async (page) => {
		await page
			.getByRole('button', { name: /show details/i })
			.first()
			.click()
			.catch(() => {});
	}
});
await shot('providers', { width: 1366, height: 560, path: '/providers' });
await shot('overview-phone', { width: 390, height: 844, scale: 2, path: '/' });
await shot('payments-phone', { width: 390, height: 844, scale: 2, path: '/payments' });
await shot('snap-choose', { width: 390, height: 844, scale: 2, path: `/snap/${chooseToken}` });
await shot('snap-va', { width: 390, height: 844, scale: 2, path: `/snap/${openToken}` });
await browser.close();
