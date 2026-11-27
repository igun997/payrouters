// Accessibility: axe (serious and critical, colour contrast included) on key pages at 390 and 1366,
// and keyboard paths through dialogs and the Snap method list.
import AxeBuilder from '@axe-core/playwright';
import { type Locator, type Page, expect } from '@playwright/test';
import { test } from './support/fixtures';
import { newSnapPayment, snapPath } from './support/snap-helpers';

/** Fails on serious and critical axe violations, one line per node with the rule, target and fix link. */
async function expectNoSeriousViolations(page: Page): Promise<void> {
	const { violations } = await new AxeBuilder({ page }).analyze();
	const blocking = violations.filter((v) => v.impact === 'serious' || v.impact === 'critical');
	const report = blocking.flatMap((v) =>
		v.nodes.map((node) =>
			[
				`${v.impact} ${v.id} at ${node.target.join(' ')}`,
				`  ${(node.failureSummary ?? v.help).split('\n').join('\n  ')}`,
				`  reference: ${v.helpUrl}`
			].join('\n')
		)
	);
	expect(report, report.join('\n\n')).toEqual([]);
}

/** Presses Tab until `target` has focus; fails when it is not reached within `max` presses. */
async function tabUntilFocused(page: Page, target: Locator, max = 25): Promise<void> {
	for (let i = 0; i < max; i++) {
		if (await target.evaluate((el) => el === document.activeElement)) return;
		await page.keyboard.press('Tab');
	}
	await expect(target).toBeFocused();
}

for (const width of [390, 1366] as const) {
	test.describe(`axe @ ${width}`, () => {
		test.use({ viewport: { width, height: width === 390 ? 844 : 768 } });

		test('Login', async ({ page }) => {
			await page.goto('/login');
			await expect(page.getByTestId('token-input')).toBeVisible();
			await expectNoSeriousViolations(page);
		});

		test('Overview', async ({ admin, api, product }) => {
			await api.paidPayment(product.sandboxKey, 150000);
			await admin.goto('/');
			await expect(admin.getByTestId('kpi-conversion')).toBeVisible();
			await expectNoSeriousViolations(admin);
		});

		test('Payments', async ({ admin, api, product }) => {
			await api.paidPayment(product.sandboxKey, 150000);
			await admin.goto('/payments');
			await expect(admin.getByTestId('payment-row').first()).toBeVisible();
			await expectNoSeriousViolations(admin);
		});

		test('Payment detail', async ({ admin, api, product }) => {
			const paid = await api.paidPayment(product.sandboxKey, 150000);
			await admin.goto(`/payments/${paid.id}`);
			await expect(admin.getByTestId('payment-order-id')).toBeVisible();
			await expectNoSeriousViolations(admin);
		});

		test('Products with the New product dialog open', async ({ admin, product }) => {
			await admin.goto('/products');
			await expect(admin.getByTestId('product-list')).toBeVisible();
			await expect(admin.getByTestId('product-list')).toContainText(product.name);
			await admin.getByTestId('new-product-button').click();
			await expect(admin.getByTestId('dialog-new-product')).toBeVisible();
			await expectNoSeriousViolations(admin);
		});

		test('API docs', async ({ admin }) => {
			await admin.goto('/docs');
			await expect(admin.getByTestId('page-docs')).toBeVisible();
			await expectNoSeriousViolations(admin);
		});

		test('Providers', async ({ admin }) => {
			await admin.goto('/providers');
			await expect(admin.getByTestId('provider-list')).toBeVisible();
			await expectNoSeriousViolations(admin);
		});

		test('Snap Choose', async ({ page, api, product }) => {
			const { token } = await newSnapPayment(api, product.sandboxKey);
			await page.goto(snapPath(token));
			await expect(page.getByTestId('snap-methods')).toBeVisible();
			await expectNoSeriousViolations(page);
		});

		test('Snap VA', async ({ page, api, product }) => {
			const { token } = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
			await page.goto(snapPath(token));
			await expect(page.getByTestId('snap-va')).toBeVisible();
			await expectNoSeriousViolations(page);
		});

		test('Snap Paid', async ({ page, api, product }) => {
			const { token } = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
			expect(await api.simulate(token, 'paid')).toBe(200);
			await page.goto(snapPath(token));
			await expect(page.getByTestId('snap-status')).toHaveAttribute('data-status', 'paid');
			await expectNoSeriousViolations(page);
		});
	});
}

test.describe('keyboard @ 1366', () => {
	test.use({ viewport: { width: 1366, height: 768 } });

	test('New product: focus enters the dialog, Tab reaches Create product, Escape returns focus to New product', async ({
		admin
	}) => {
		await admin.goto('/products');
		const trigger = admin.getByTestId('new-product-button');
		await expect(trigger).toBeVisible();
		await trigger.focus();
		await admin.keyboard.press('Enter');

		const dialog = admin.getByTestId('dialog-new-product');
		await expect(dialog).toBeVisible();
		expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);

		await tabUntilFocused(
			admin,
			dialog.getByRole('button', { name: 'Create product', exact: true })
		);
		await admin.keyboard.press('Escape');
		await expect(dialog).toBeHidden();
		await expect(trigger).toBeFocused();
	});

	test('Add account: Tab reaches Save account, Escape closes it and returns focus to Add account', async ({
		admin
	}) => {
		await admin.goto('/providers');
		const trigger = admin.getByTestId('add-provider-button');
		await expect(trigger).toBeEnabled();
		await trigger.focus();
		await admin.keyboard.press('Enter');

		const dialog = admin.getByTestId('dialog-add-provider');
		await expect(dialog).toBeVisible();
		await tabUntilFocused(admin, dialog.getByRole('button', { name: 'Save account', exact: true }));
		await admin.keyboard.press('Escape');
		await expect(dialog).toBeHidden();
		await expect(trigger).toBeFocused();
	});

	test('Live mode: Tab reaches the confirm action, Escape closes it and returns focus to the Live option', async ({
		admin
	}) => {
		await admin.goto('/');
		const live = admin.getByTestId('mode-toggle').locator('button', { hasText: 'Live' });
		await expect(live).toBeVisible();
		await live.focus();
		await admin.keyboard.press('Enter');

		const dialog = admin.getByTestId('dialog-live-mode');
		await expect(dialog).toBeVisible();
		await tabUntilFocused(admin, admin.getByTestId('mode-confirm-live'));
		await admin.keyboard.press('Escape');
		await expect(dialog).toBeHidden();
		await expect(live).toBeFocused();
	});

	test('Snap method rows are reachable with Tab and activate with Enter', async ({
		page,
		api,
		product
	}) => {
		const { token } = await newSnapPayment(api, product.sandboxKey);
		await page.goto(snapPath(token));
		const bca = page.getByTestId('snap-method-va_bca');
		await tabUntilFocused(page, bca, 10);
		await page.keyboard.press('Enter');
		await expect(page.getByTestId('snap-va')).toBeVisible();
	});
});
