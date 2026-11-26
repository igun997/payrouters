// Xendit callbacks (x-callback-token), the Midtrans terminal notifications, and the IDRX client-side
// rules. Xendit payments are created without a method (snap checkout, no provider call), so the
// callback path is exercised offline; the provider never sees these requests.
import type { APIRequestContext, Page } from '@playwright/test';
import { ADMIN_TOKEN, Api, XENDIT_CALLBACK_TOKEN, uid } from './support/api';
import { expect, test } from './support/fixtures';
import { postMidtransNotification, switchModeDesktop } from './support/flows';

async function postXendit(
	req: APIRequestContext,
	accountId: string,
	body: unknown,
	token: string | null
): Promise<number> {
	const res = await req.post(`/hooks/${accountId}`, {
		headers: token === null ? {} : { 'x-callback-token': token },
		data: body
	});
	return res.status();
}

/** A Xendit-provider payment in snap checkout with no method chosen: no provider call is made. */
async function xenditPayment(api: Api, key: string, amount = 150000): Promise<string> {
	const created = await api.createPayment(key, {
		order_id: uid('XND'),
		amount,
		provider: 'xendit'
	});
	expect(created.status).toBe(201);
	return created.json.id;
}

/** Inbound rows (newest first) that point at one payment. */
function inboundRows(page: Page, paymentId: string) {
	return page.getByTestId('webhook-event-row').filter({
		has: page.locator(`a[href$="/payments/${paymentId}"]`)
	});
}

const eventBody = (event: string, data: Record<string, unknown>) => ({ event, data });

test.describe('Xendit callbacks', () => {
	test('a wrong x-callback-token is rejected with 401 and shown as not verified', async ({
		admin,
		api,
		product,
		request
	}) => {
		const xendit = await api.ensureProvider('xendit');
		const id = await xenditPayment(api, product.sandboxKey);
		const status = await postXendit(
			request,
			xendit.id,
			eventBody('payment.capture', {
				reference_id: id,
				status: 'SUCCEEDED',
				request_amount: 150000
			}),
			'not-the-token'
		);
		expect(status).toBe(401);

		await admin.goto('/webhooks');
		const row = admin.getByTestId('webhook-event-row').first();
		await expect(row).toHaveAttribute('data-verified', 'false');
		await expect(row.getByTestId('outcome-pill')).toHaveAttribute('data-outcome', 'rejected');
		await expect(row).toContainText('x-callback-token mismatch');
	});

	test('the correct token applies payment.capture SUCCEEDED when the amount matches', async ({
		admin,
		api,
		product,
		request
	}) => {
		const xendit = await api.ensureProvider('xendit');
		const id = await xenditPayment(api, product.sandboxKey, 150000);
		const status = await postXendit(
			request,
			xendit.id,
			eventBody('payment.capture', {
				reference_id: id,
				status: 'SUCCEEDED',
				request_amount: 150000,
				channel_code: 'QRIS',
				captures: [{ capture_timestamp: new Date().toISOString() }]
			}),
			XENDIT_CALLBACK_TOKEN
		);
		expect(status).toBe(200);

		await admin.goto('/webhooks');
		const row = inboundRows(admin, id);
		await expect(row).toHaveCount(1);
		await expect(row).toHaveAttribute('data-verified', 'true');
		await expect(row.getByTestId('outcome-pill')).toHaveAttribute('data-outcome', 'applied');

		await admin.goto(`/payments/${id}`);
		await expect(
			admin.getByTestId('page-payment-detail').getByTestId('status-badge').first()
		).toHaveAttribute('data-status', 'paid');
	});

	test('payment_session.expired expires its payment', async ({ admin, api, product, request }) => {
		const xendit = await api.ensureProvider('xendit');
		const id = await xenditPayment(api, product.sandboxKey);
		const status = await postXendit(
			request,
			xendit.id,
			eventBody('payment_session.expired', {
				reference_id: id,
				status: 'EXPIRED',
				amount: 150000,
				updated: new Date().toISOString()
			}),
			XENDIT_CALLBACK_TOKEN
		);
		expect(status).toBe(200);

		await admin.goto(`/payments/${id}`);
		await expect(
			admin.getByTestId('page-payment-detail').getByTestId('status-badge').first()
		).toHaveAttribute('data-status', 'expired');
	});

	test('a capture for a different amount is rejected and the reason is shown on the event', async ({
		admin,
		api,
		product,
		request
	}) => {
		const xendit = await api.ensureProvider('xendit');
		const id = await xenditPayment(api, product.sandboxKey, 150000);
		const status = await postXendit(
			request,
			xendit.id,
			eventBody('payment.capture', {
				reference_id: id,
				status: 'SUCCEEDED',
				request_amount: 149000
			}),
			XENDIT_CALLBACK_TOKEN
		);
		expect(status).toBe(200);

		await admin.goto('/webhooks');
		const row = inboundRows(admin, id);
		await expect(row.getByTestId('outcome-pill')).toHaveAttribute('data-outcome', 'rejected');
		await expect(row).toContainText('amount mismatch: provider reports 149000, payment is 150000');

		await admin.goto(`/payments/${id}`);
		await expect(
			admin.getByTestId('page-payment-detail').getByTestId('status-badge').first()
		).toHaveAttribute('data-status', 'pending');
	});

	test('an event for an unknown reference is stored as unmatched', async ({
		admin,
		api,
		request
	}) => {
		const xendit = await api.ensureProvider('xendit');
		const status = await postXendit(
			request,
			xendit.id,
			eventBody('payment.capture', {
				reference_id: crypto.randomUUID(),
				status: 'SUCCEEDED',
				request_amount: 150000
			}),
			XENDIT_CALLBACK_TOKEN
		);
		expect(status).toBe(200);

		await admin.goto('/webhooks');
		const row = admin.getByTestId('webhook-event-row').first();
		await expect(row.getByTestId('outcome-pill')).toHaveAttribute('data-outcome', 'unmatched');
		await expect(row.locator('a[href*="/payments/"]')).toHaveCount(0);
	});

	test('a legacy invoice-shaped callback with a valid token is acknowledged as a noop', async ({
		admin,
		api,
		request
	}) => {
		const xendit = await api.ensureProvider('xendit');
		const status = await postXendit(
			request,
			xendit.id,
			{ id: 'inv_e2e_legacy', external_id: uid('LEG'), status: 'PAID', amount: 150000 },
			XENDIT_CALLBACK_TOKEN
		);
		expect(status).toBe(200);

		await admin.goto('/webhooks');
		const row = admin.getByTestId('webhook-event-row').first();
		await expect(row.getByTestId('outcome-pill')).toHaveAttribute('data-outcome', 'noop');
	});

	test('the stored x-callback-token header is redacted and the secret never reaches the page', async ({
		admin,
		api,
		product,
		request
	}) => {
		const xendit = await api.ensureProvider('xendit');
		const id = await xenditPayment(api, product.sandboxKey);
		await postXendit(
			request,
			xendit.id,
			eventBody('payment.capture', {
				reference_id: id,
				status: 'SUCCEEDED',
				request_amount: 150000
			}),
			XENDIT_CALLBACK_TOKEN
		);

		await admin.goto('/webhooks');
		const row = inboundRows(admin, id);
		await row.getByRole('button').first().click();
		const detail = row.getByTestId('webhook-event-detail');
		await expect(detail).toContainText('x-callback-token');
		await expect(detail).toContainText('[redacted]');

		expect(await admin.content()).not.toContain(XENDIT_CALLBACK_TOKEN);
	});
});

test.describe('Midtrans terminal notifications', () => {
	const cases = [
		{ status: 'expire', badge: 'expired', label: 'Expired' },
		{ status: 'cancel', badge: 'cancelled', label: 'Cancelled' },
		{ status: 'deny', badge: 'failed', label: 'Failed' }
	] as const;

	for (const c of cases) {
		test(`a ${c.status} notification sets the payment to ${c.badge}`, async ({
			admin,
			api,
			product,
			request
		}) => {
			const midtrans = await api.ensureProvider('midtrans');
			const payment = await api.createPayment(product.sandboxKey, {
				order_id: uid('MT-TERM'),
				amount: 150000,
				provider: 'midtrans'
			});
			expect(
				await postMidtransNotification(request, midtrans.id, payment.json.id, 150000, {
					status: c.status
				})
			).toBe(200);

			await admin.goto(`/payments/${payment.json.id}`);
			const badge = admin.getByTestId('page-payment-detail').getByTestId('status-badge').first();
			await expect(badge).toHaveAttribute('data-status', c.badge);
			await expect(badge).toContainText(c.label);
		});
	}

	test('a late settlement after expire flips the payment to paid', async ({
		admin,
		api,
		product,
		request
	}) => {
		const midtrans = await api.ensureProvider('midtrans');
		const payment = await api.createPayment(product.sandboxKey, {
			order_id: uid('MT-LATE'),
			amount: 150000,
			provider: 'midtrans'
		});
		await postMidtransNotification(request, midtrans.id, payment.json.id, 150000, {
			status: 'expire'
		});
		await admin.goto(`/payments/${payment.json.id}`);
		const badge = admin.getByTestId('page-payment-detail').getByTestId('status-badge').first();
		await expect(badge).toHaveAttribute('data-status', 'expired');

		expect(
			await postMidtransNotification(request, midtrans.id, payment.json.id, 150000, {
				status: 'settlement'
			})
		).toBe(200);
		await admin.reload();
		await expect(badge).toHaveAttribute('data-status', 'paid');
	});
});

test.describe('IDRX', () => {
	test('IDRX is live only: the account is listed, and the sandbox API refuses IDRX payments', async ({
		admin,
		request,
		product
	}) => {
		const existing = await request.get('/admin/providers', {
			headers: { Authorization: `Bearer ${ADMIN_TOKEN}` }
		});
		const accounts = (await existing.json()) as { data: { provider: string; mode: string }[] };
		if (!accounts.data.some((a) => a.provider === 'idrx' && a.mode === 'live')) {
			const created = await request.post('/admin/providers', {
				headers: { Authorization: `Bearer ${ADMIN_TOKEN}` },
				data: {
					provider: 'idrx',
					mode: 'live',
					credentials: {
						api_key: 'idrx_e2e_fake_key',
						api_secret: 'idrx_e2e_fake_secret',
						default_wallet_address: `0x${'a'.repeat(40)}`,
						default_chain_id: '8453'
					}
				}
			});
			expect(created.status()).toBe(201);
		}

		await admin.goto('/providers');
		await expect(admin.getByTestId('provider-list')).toContainText('IDRX');

		const res = await request.post('/v1/payments', {
			headers: { Authorization: `Bearer ${product.sandboxKey}` },
			data: { order_id: uid('IDRX'), amount: 150000, provider: 'idrx' }
		});
		expect(res.status()).toBe(422);
		const body = (await res.json()) as { error: { code: string } };
		expect(body.error.code).toBe('sandbox_unsupported');

		// The Playground lists only enabled accounts of the current mode, so IDRX is not offered in sandbox.
		await admin.goto('/playground');
		await admin.getByRole('combobox', { name: 'Choose a product', exact: true }).click();
		await admin.getByRole('option', { name: product.name, exact: true }).click();
		await admin.getByRole('combobox', { name: 'Provider', exact: true }).click();
		await expect(admin.getByRole('option', { name: /IDRX/ })).toHaveCount(0);
		await admin.keyboard.press('Escape');
	});

	test('IDRX amounts below Rp 20.000 are rejected in the Playground before any request', async ({
		admin,
		product
	}) => {
		await admin.goto('/playground');
		await switchModeDesktop(admin, 'live');
		await admin.getByRole('combobox', { name: 'Choose a product', exact: true }).click();
		await admin.getByRole('option', { name: product.name, exact: true }).click();
		await admin.getByRole('combobox', { name: 'Provider', exact: true }).click();
		await admin.getByRole('option', { name: /IDRX/ }).click();

		await admin.getByLabel('Amount', { exact: true }).fill('19999');
		await admin.getByTestId('create-payment').click();
		await expect(admin.getByTestId('field-pg-amount').getByRole('alert')).toContainText(
			'IDRX mints at least Rp 20.000'
		);
		await expect(admin.getByTestId('payment-result')).toHaveCount(0);
	});
});
