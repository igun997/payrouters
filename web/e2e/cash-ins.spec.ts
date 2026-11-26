// Cash-in recap: totals for one product's paid payments, sandbox vs live isolation, CSV export.
import { readFileSync } from 'node:fs';
import type { Locator, Page } from '@playwright/test';
import { uid } from './support/api';
import { expect, test } from './support/fixtures';
import { getPaymentDetail, rp, switchModeDesktop } from './support/flows';

const CSV_HEADER =
	'paid_at,order_id,payment_id,product,provider,method,mode,gross,fee,net,provider_ref';

/** The value under a label in the totals strip (Payments, Gross, Fees, Net received). */
function totalOf(page: Page, label: string): Locator {
	return page
		.getByTestId('cash-in-totals')
		.locator('div')
		.filter({ has: page.locator('dt', { hasText: label }) })
		.locator('dd');
}

async function chooseProduct(page: Page, name: string) {
	await page.getByRole('combobox', { name: 'Product' }).click();
	await page.getByRole('option', { name: name, exact: true }).click();
}

test.describe('cash-ins', () => {
	test('totals equal the paid payments of the product', async ({
		admin,
		api,
		product,
		request
	}) => {
		const paid = [];
		for (const amount of [100000, 250000, 50000]) {
			paid.push(await api.paidPayment(product.sandboxKey, amount));
		}
		// Pending payments are not cash-ins and must not move the totals.
		await api.createPayment(product.sandboxKey, {
			order_id: uid('OPEN'),
			amount: 70000,
			provider: 'mock'
		});

		const cashIns = (await Promise.all(paid.map((p) => getPaymentDetail(request, p.id)))).map(
			(d) => d.cash_in!
		);
		const sum = (pick: (c: (typeof cashIns)[number]) => number) =>
			cashIns.reduce((total, c) => total + pick(c), 0);

		await admin.goto('/cash-ins');
		await chooseProduct(admin, product.name);

		await expect(totalOf(admin, 'Payments')).toHaveText('3');
		await expect(totalOf(admin, 'Gross')).toHaveText(rp(sum((c) => c.gross_amount)));
		await expect(totalOf(admin, 'Fees')).toHaveText(rp(sum((c) => c.fee_amount)));
		await expect(totalOf(admin, 'Net received')).toHaveText(rp(sum((c) => c.net_amount)));
		await expect(admin.getByTestId('cash-in-row')).toHaveCount(3);
	});

	test('the live mode shows no sandbox cash-ins, and switching back restores them', async ({
		admin,
		api,
		product
	}) => {
		for (const amount of [100000, 40000]) await api.paidPayment(product.sandboxKey, amount);

		await admin.goto('/cash-ins');
		await chooseProduct(admin, product.name);
		await expect(totalOf(admin, 'Payments')).toHaveText('2');

		await switchModeDesktop(admin, 'live');
		await expect(totalOf(admin, 'Payments')).toHaveText('0');
		await expect(admin.getByText('No cash-ins in this range')).toBeVisible();

		await switchModeDesktop(admin, 'sandbox');
		await expect(totalOf(admin, 'Payments')).toHaveText('2');
	});

	test('CSV export downloads the header and one row per cash-in', async ({
		admin,
		api,
		product,
		request
	}) => {
		const paid = [];
		for (const amount of [120000, 30000]) {
			paid.push(await api.paidPayment(product.sandboxKey, amount));
		}
		await admin.goto('/cash-ins');
		await chooseProduct(admin, product.name);
		await expect(totalOf(admin, 'Payments')).toHaveText('2');

		const [download] = await Promise.all([
			admin.waitForEvent('download'),
			admin.getByTestId('export-csv').click()
		]);
		expect(download.suggestedFilename()).toMatch(/^payrouters-cash-ins-sandbox-.*\.csv$/);

		const lines = readFileSync((await download.path()) ?? '', 'utf8')
			.trim()
			.split('\n');
		expect(lines[0]).toBe(CSV_HEADER);
		expect(lines.length - 1).toBe(2);

		const target = paid[0];
		const detail = await getPaymentDetail(request, target.id);
		const row = lines.find((line) => line.split(',')[1] === target.orderId);
		expect(row).toBeDefined();
		const cells = row!.split(',');
		expect(cells[2]).toBe(target.id);
		expect(cells[3]).toBe(product.name);
		expect(cells[4]).toBe('mock');
		expect(cells[6]).toBe('sandbox');
		expect(cells[7]).toBe(String(detail.cash_in!.gross_amount));
		expect(cells[8]).toBe(String(detail.cash_in!.fee_amount));
		expect(cells[9]).toBe(String(detail.cash_in!.net_amount));
	});
});
