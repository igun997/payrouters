// Playground: create a mock sandbox payment from the form, validation, the generated order ID, the
// result card and the Snap page it opens. Phone: the create bar stays sticky above the tab bar.
import type { Page } from '@playwright/test';
import { expect, test } from './support/fixtures';
import { rp } from './support/flows';

async function choose(page: Page, trigger: string, option: string) {
	await page.getByRole('combobox', { name: trigger, exact: true }).click();
	await page.getByRole('option', { name: option, exact: true }).click();
}

test.describe('playground', () => {
	test('creates a mock sandbox payment and opens its Snap page', async ({ admin, product }) => {
		await admin.goto('/playground');
		await choose(admin, 'Choose a product', product.name);
		await choose(admin, 'Provider', 'Mock simulator');

		const orderId = admin.getByLabel('Order ID', { exact: true });
		await expect(orderId).toHaveValue(/^PG-[0-9A-F]{8}$/);
		const generated = await orderId.inputValue();
		await expect(admin.getByLabel('Amount', { exact: true })).toHaveValue('150.000');

		await admin.getByTestId('create-payment').click();

		const result = admin.getByTestId('payment-result');
		await expect(result).toHaveAttribute('data-status', 'pending');
		await expect(result).toContainText(generated);
		await expect(result).toContainText(rp(150000));
		await expect(admin.getByTestId('recent-list')).toContainText(generated);

		const popupPage = admin.waitForEvent('popup');
		await result.getByRole('link', { name: 'Open Snap page' }).click();
		const snap = await popupPage;
		await expect(snap.getByTestId('snap-amount')).toContainText(rp(150000));
		await expect(snap.getByTestId('snap-methods')).toBeVisible();
	});

	test('a zero amount is rejected and an order ID can be regenerated', async ({ admin }) => {
		await admin.goto('/playground');
		const amount = admin.getByLabel('Amount', { exact: true });
		await amount.fill('0');
		await admin.getByTestId('create-payment').click();

		await expect(admin.getByTestId('field-pg-amount').getByRole('alert')).toContainText(
			'Enter an amount of at least Rp 1'
		);
		await expect(admin.getByTestId('payment-result')).toHaveCount(0);

		const orderId = admin.getByLabel('Order ID', { exact: true });
		const before = await orderId.inputValue();
		await admin.getByRole('button', { name: 'New order ID' }).click();
		await expect(orderId).not.toHaveValue(before);
		await expect(orderId).toHaveValue(/^PG-[0-9A-F]{8}$/);
	});

	test('keeps the create bar sticky above the tab bar @phone', async ({ admin }, testInfo) => {
		test.skip(testInfo.project.name !== 'phone', 'phone layout only');
		await admin.goto('/playground');

		const bar = admin.getByTestId('create-bar');
		await expect(bar).toBeVisible();
		expect(await bar.evaluate((el) => getComputedStyle(el).position)).toBe('fixed');

		const barBox = await bar.boundingBox();
		const tabBox = await admin.getByTestId('tab-bar').boundingBox();
		expect(barBox).not.toBeNull();
		expect(tabBox).not.toBeNull();
		expect(barBox!.y + barBox!.height).toBeLessThanOrEqual(tabBox!.y + 1);

		await expect(admin.getByTestId('create-payment')).toBeInViewport();
	});
});
