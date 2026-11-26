// Payments: list order, status chips, search, cursor pager, detail ledger with a 0.7% mock fee,
// simulator, sync and copying the Snap link. Each test searches on its own unique order prefix.
import type { Page } from '@playwright/test';
import { uid } from './support/api';
import { expect, hasHorizontalScroll, test } from './support/fixtures';
import { grantClipboard, percentFee, readClipboard, rp } from './support/flows';

async function search(page: Page, text: string) {
	await page.getByTestId('payments-search').fill(text);
}

test.describe('payments', () => {
	test('lists created payments newest first', async ({ admin, api, product }) => {
		const tag = uid('LIST');
		for (const n of [1, 2, 3]) {
			await api.createPayment(product.sandboxKey, {
				order_id: `${tag}-${n}`,
				amount: 10000 * n,
				provider: 'mock'
			});
		}

		await admin.goto('/payments');
		await search(admin, tag);
		const rows = admin.getByTestId('payment-row');
		await expect(rows).toHaveCount(3);
		await expect(rows.nth(0)).toContainText(`${tag}-3`);
		await expect(rows.nth(1)).toContainText(`${tag}-2`);
		await expect(rows.nth(2)).toContainText(`${tag}-1`);
	});

	test('status chips narrow the list to one status', async ({ admin, api, product }) => {
		const tag = uid('CHIP');
		const paid = await api.paidPayment(product.sandboxKey, 50000, 'va_bca', `${tag}-PAID`);
		await api.createPayment(product.sandboxKey, {
			order_id: `${tag}-OPEN`,
			amount: 60000,
			provider: 'mock'
		});

		await admin.goto('/payments');
		await search(admin, tag);
		const rows = admin.getByTestId('payment-row');
		await expect(rows).toHaveCount(2);

		await admin.getByTestId('status-chip-paid').click();
		await expect(rows).toHaveCount(1);
		await expect(rows.first()).toContainText(paid.orderId);
		await expect(rows.first().getByTestId('status-badge')).toHaveAttribute('data-status', 'paid');

		await admin.getByTestId('status-chip-pending').click();
		await expect(rows).toHaveCount(1);
		await expect(rows.first()).toContainText(`${tag}-OPEN`);

		await admin.getByTestId('status-chip-all').click();
		await expect(rows).toHaveCount(2);
	});

	test('search finds one payment by its full order ID', async ({ admin, api, product }) => {
		const tag = uid('SRCH');
		await api.createPayment(product.sandboxKey, {
			order_id: `${tag}-A`,
			amount: 1000,
			provider: 'mock'
		});
		await api.createPayment(product.sandboxKey, {
			order_id: `${tag}-B`,
			amount: 2000,
			provider: 'mock'
		});

		await admin.goto('/payments');
		await search(admin, `${tag}-A`);
		const rows = admin.getByTestId('payment-row');
		await expect(rows).toHaveCount(1);
		await expect(rows.first()).toContainText(`${tag}-A`);

		await search(admin, `none-${tag}`);
		await expect(admin.getByText('No payments match these filters')).toBeVisible();
	});

	test('pages through more than one page with the cursor pager', async ({
		admin,
		api,
		product
	}) => {
		const tag = uid('PAGE');
		for (let i = 1; i <= 26; i++) {
			await api.createPayment(product.sandboxKey, {
				order_id: `${tag}-${String(i).padStart(2, '0')}`,
				amount: 1000 + i,
				provider: 'mock'
			});
		}

		await admin.goto('/payments');
		await search(admin, tag);
		const rows = admin.getByTestId('payment-row');
		const pager = admin.getByTestId('pager');
		// Without the search the newest 25 payments (ours, just created) are already listed, so wait
		// for the search to apply: a filtered list shows no total, an unfiltered one shows "of N".
		await expect(admin.getByTestId('pager-status')).toHaveText(/^Showing 1\D+25$/);
		await expect(rows).toHaveCount(25);
		await expect(rows.first()).toContainText(`${tag}-26`);

		await expect(pager.getByRole('button', { name: 'Next' })).toBeEnabled();
		await pager.getByRole('button', { name: 'Next' }).click();
		await expect(rows).toHaveCount(1);
		await expect(rows.first()).toContainText(`${tag}-01`);
		await expect(admin.getByTestId('pager-status')).toContainText(/Showing 26\D+26/);

		await pager.getByRole('button', { name: 'Previous' }).click();
		await expect(rows).toHaveCount(25);
		await expect(rows.first()).toContainText(`${tag}-26`);
	});

	test('detail ledger shows gross, fee and net after payment with a 0.7% mock fee', async ({
		admin,
		api,
		product
	}) => {
		await api.setMockFees({ '*': { fixed: 0, percent: 0.7 } });
		const paid = await api.paidPayment(product.sandboxKey, 150000);
		const fee = percentFee(150000, 0.7);

		await admin.goto(`/payments/${paid.id}`);
		await expect(admin.getByTestId('payment-order-id')).toHaveText(paid.orderId);
		const ledger = admin.getByTestId('cash-in-ledger');
		await expect(ledger).toContainText(rp(150000));
		await expect(ledger).toContainText(rp(fee));
		await expect(ledger).toContainText(rp(150000 - fee));
	});

	test('simulates paid, expired and failed from the detail page', async ({
		admin,
		api,
		product
	}) => {
		const outcomes = [
			{ button: 'Simulate paid', status: 'paid' },
			{ button: 'Simulate expired', status: 'expired' },
			{ button: 'Simulate failed', status: 'failed' }
		] as const;

		for (const { button, status } of outcomes) {
			const pending = await api.createPayment(product.sandboxKey, {
				order_id: uid('SIM'),
				amount: 50000,
				provider: 'mock',
				method: 'va_bca'
			});
			await admin.goto(`/payments/${pending.json.id}`);
			const simulator = admin.getByTestId('simulator');
			await expect(simulator).toBeVisible();

			await simulator.getByRole('button', { name: button }).click();
			await expect(admin.getByText(`Simulated ${status}`)).toBeVisible();
			await expect(
				admin.getByTestId('page-payment-detail').getByTestId('status-badge').first()
			).toHaveAttribute('data-status', status);
			await expect(simulator).toBeHidden();
		}
	});

	test('sync with the mock provider keeps a paid payment paid', async ({ admin, api, product }) => {
		const paid = await api.paidPayment(product.sandboxKey, 80000);
		await admin.goto(`/payments/${paid.id}`);
		const badge = admin.getByTestId('page-payment-detail').getByTestId('status-badge').first();
		await expect(badge).toHaveAttribute('data-status', 'paid');

		await admin.getByTestId('sync-payment').click();
		await expect(admin.getByText('Payment synced with provider')).toBeVisible();
		await expect(badge).toHaveAttribute('data-status', 'paid');
	});

	test('copies the full Snap link of a payment', async ({ admin, api, product }) => {
		await grantClipboard(admin);
		const pending = await api.createPayment(product.sandboxKey, {
			order_id: uid('SNAP'),
			amount: 20000,
			provider: 'mock'
		});

		await admin.goto(`/payments/${pending.json.id}`);
		const link = admin.getByTestId('snap-link');
		await expect(link).toBeVisible();
		await link.getByRole('button', { name: 'Copy', exact: true }).click();
		await expect(admin.getByText('Copied to clipboard')).toBeVisible();
		expect(await readClipboard(admin)).toBe(pending.json.snap_url);
	});

	test('renders payments as cards and a readable detail @phone', async ({
		admin,
		api,
		product
	}, testInfo) => {
		test.skip(testInfo.project.name !== 'phone', 'phone layout only');
		const paid = await api.paidPayment(product.sandboxKey, 75000);

		await admin.goto('/payments');
		await search(admin, paid.orderId);
		await expect(admin.getByRole('table')).toHaveCount(0);
		const list = admin.getByTestId('payments-list');
		await expect(list).toBeVisible();
		expect(await list.evaluate((el) => el.tagName)).toBe('UL');

		const card = admin.getByTestId('payment-row').filter({ hasText: paid.orderId });
		await expect(card).toContainText(product.name);
		await expect(card).toContainText(rp(75000));
		await card.getByRole('link').click();

		await expect(admin).toHaveURL(/\/payments\/[0-9a-f-]+$/);
		await expect(admin.getByTestId('payment-summary')).toContainText(rp(75000));
		await expect(admin.getByTestId('cash-in-ledger')).toContainText(rp(75000));
		expect(await hasHorizontalScroll(admin)).toBe(false);
	});
});
