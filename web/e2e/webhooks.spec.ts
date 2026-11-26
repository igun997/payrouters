// Webhooks. Inbound: a signed Midtrans notification is applied, a forged one is rejected, an
// unknown order is unmatched. Outbound: a paid event reaches a local receiver with a valid
// PayRouters-Signature; a failing receiver shows attempts and Retry delivers after recovery.
import { randomUUID } from 'node:crypto';
import type { Page } from '@playwright/test';
import { uid } from './support/api';
import { expect, test } from './support/fixtures';
import {
	getPaymentDetail,
	listDeliveries,
	postMidtransNotification,
	startReceiver,
	verifyPayRoutersSignature
} from './support/flows';

async function openOutbound(page: Page, productName: string) {
	await page.goto('/webhooks');
	await page.getByRole('tab', { name: 'Outbound deliveries' }).click();
	await page.getByTestId('filter-product').getByRole('combobox', { name: 'Product' }).click();
	await page.getByRole('option', { name: productName, exact: true }).click();
}

async function filterInbound(page: Page, outcome: string) {
	await page.goto('/webhooks');
	await page.getByTestId('filter-outcome').getByRole('combobox', { name: 'Outcome' }).click();
	await page.getByRole('option', { name: outcome, exact: true }).click();
}

test.describe('webhooks', () => {
	test('a signed Midtrans notification is applied and settles the payment', async ({
		admin,
		api,
		product
	}) => {
		const midtrans = await api.ensureProvider('midtrans');
		const payment = await api.createPayment(product.sandboxKey, {
			order_id: uid('MT-HOOK'),
			amount: 150000,
			provider: 'midtrans'
		});
		expect(await api.midtransNotification(midtrans, payment.json.id, 150000)).toBe(200);

		await admin.goto('/webhooks');
		const row = admin.getByTestId('webhook-event-row').filter({
			has: admin.locator(`a[href$="/payments/${payment.json.id}"]`)
		});
		await expect(row).toHaveCount(1);
		await expect(row).toHaveAttribute('data-verified', 'true');
		await expect(row.getByTestId('outcome-pill')).toHaveAttribute('data-outcome', 'applied');
		await expect(row).toContainText('Verified');

		await row.getByTestId('webhook-payment-link').click();
		await expect(
			admin.getByTestId('page-payment-detail').getByTestId('status-badge').first()
		).toHaveAttribute('data-status', 'paid');
	});

	test('a forged signature is rejected, shown as not verified, and settles nothing', async ({
		admin,
		api,
		product,
		request
	}) => {
		const midtrans = await api.ensureProvider('midtrans');
		const payment = await api.createPayment(product.sandboxKey, {
			order_id: uid('MT-FORGE'),
			amount: 150000,
			provider: 'midtrans'
		});

		const status = await postMidtransNotification(request, midtrans.id, payment.json.id, 150000, {
			signature: 'f'.repeat(128)
		});
		expect(status).toBe(401);

		await filterInbound(admin, 'Rejected');
		const row = admin.getByTestId('webhook-event-row').first();
		await expect(row.getByTestId('outcome-pill')).toHaveAttribute('data-outcome', 'rejected');
		await expect(row).toHaveAttribute('data-verified', 'false');
		await expect(row).toContainText('Not verified');

		await row.getByRole('button', { name: 'Show details' }).click();
		await expect(row.getByTestId('webhook-event-detail')).toContainText(payment.json.id);

		const detail = await getPaymentDetail(request, payment.json.id);
		expect(detail.payment.status).toBe('pending');
	});

	test('a notification for an unknown order is stored as unmatched', async ({ admin, api }) => {
		const midtrans = await api.ensureProvider('midtrans');
		const unknown = randomUUID();
		expect(await api.midtransNotification(midtrans, unknown, 150000)).toBe(200);

		await filterInbound(admin, 'Unmatched');
		const row = admin.getByTestId('webhook-event-row').first();
		await expect(row.getByTestId('outcome-pill')).toHaveAttribute('data-outcome', 'unmatched');
		await expect(row).toHaveAttribute('data-verified', 'true');
		await expect(row.getByTestId('webhook-payment-link')).toHaveCount(0);

		await row.getByRole('button', { name: 'Show details' }).click();
		await expect(row.getByTestId('webhook-event-detail')).toContainText(unknown);
	});

	test('a paid event reaches the product webhook with a valid signature', async ({
		admin,
		api,
		request
	}) => {
		const receiver = await startReceiver(200);
		try {
			const product = await api.createProduct(uid('Hook'), receiver.url);
			const paid = await api.paidPayment(product.sandboxKey, 150000);

			await expect.poll(() => receiver.requests.length, { timeout: 15_000 }).toBeGreaterThan(0);
			const delivery = receiver.requests[0];
			expect(delivery.method).toBe('POST');
			expect(delivery.headers['content-type']).toBe('application/json');
			expect(delivery.headers['payrouters-event-id']).toMatch(/^evt_/);
			expect(
				verifyPayRoutersSignature(
					product.webhookSecret,
					delivery.headers['payrouters-signature'] as string | undefined,
					delivery.body
				)
			).toBe(true);

			const body = JSON.parse(delivery.body) as {
				id: string;
				type: string;
				data: { payment: { id: string } };
			};
			expect(body.type).toBe('payment.paid');
			expect(body.id).toBe(delivery.headers['payrouters-event-id']);
			expect(body.data.payment.id).toBe(paid.id);

			await openOutbound(admin, product.name);
			const row = admin.getByTestId('delivery-row');
			await expect(row).toHaveCount(1);
			await expect(row.getByTestId('delivery-status')).toHaveAttribute('data-status', 'delivered');
			await expect(row).toContainText('payment.paid');
			await expect(row).toContainText('HTTP 200');
			expect((await listDeliveries(request, product.id))[0].status).toBe('delivered');
		} finally {
			await receiver.close();
		}
	});

	test('a failing receiver shows attempts, and Retry delivers once it recovers', async ({
		admin,
		api,
		request
	}) => {
		const receiver = await startReceiver(500);
		try {
			const product = await api.createProduct(uid('Flaky'), receiver.url);
			await api.paidPayment(product.sandboxKey, 90000);

			await expect
				.poll(async () => (await listDeliveries(request, product.id))[0]?.attempts ?? 0, {
					timeout: 15_000
				})
				.toBeGreaterThanOrEqual(1);
			const failed = (await listDeliveries(request, product.id))[0];
			expect(failed.status).toBe('pending');
			expect(failed.last_status_code).toBe(500);

			await openOutbound(admin, product.name);
			const row = admin.getByTestId('delivery-row');
			await expect(row).toHaveCount(1);
			await expect(row.getByTestId('delivery-status')).toHaveAttribute('data-status', 'pending');
			await expect(row).toContainText('1/8 attempts');
			await expect(row).toContainText('HTTP 500');

			receiver.setStatus(200);
			await row.getByTestId('retry-delivery').click();
			await expect(admin.getByText('Delivery queued for retry')).toBeVisible();

			await expect
				.poll(async () => (await listDeliveries(request, product.id))[0]?.status, {
					timeout: 15_000
				})
				.toBe('delivered');

			await openOutbound(admin, product.name);
			await expect(
				admin.getByTestId('delivery-row').getByTestId('delivery-status').first()
			).toHaveAttribute('data-status', 'delivered');
		} finally {
			await receiver.close();
		}
	});
});
