// Dark mode: the derived palette must keep text readable (WCAG AA 4.5:1 via axe colour-contrast) on the
// key pages and the Products dialog, at phone and desktop widths. The theme follows the system scheme.
import AxeBuilder from '@axe-core/playwright';
import { type Page, expect } from '@playwright/test';
import { test } from './support/fixtures';
import { newSnapPayment, snapPath } from './support/snap-helpers';

/** Fails on serious and critical colour-contrast violations, one line per node with its colours. */
async function expectReadableContrast(page: Page): Promise<void> {
	await expect(page.locator('html')).toHaveClass(/\bdark\b/);
	const { violations } = await new AxeBuilder({ page }).withRules(['color-contrast']).analyze();
	const blocking = violations.filter((v) => v.impact === 'serious' || v.impact === 'critical');
	const report = blocking.flatMap((v) =>
		v.nodes.map((node) => `${node.target.join(' ')}: ${node.failureSummary ?? v.help}`)
	);
	expect(report, report.join('\n')).toEqual([]);
}

for (const width of [390, 1366] as const) {
	test.describe(`dark contrast @ ${width}`, () => {
		test.use({
			viewport: { width, height: width === 390 ? 844 : 768 },
			colorScheme: 'dark'
		});

		test('Overview', async ({ admin, api, product }) => {
			await api.paidPayment(product.sandboxKey, 150000);
			await admin.goto('/');
			await expect(admin.getByTestId('kpi-conversion')).toBeVisible();
			await expectReadableContrast(admin);
		});

		test('Payments', async ({ admin, api, product }) => {
			await api.paidPayment(product.sandboxKey, 150000);
			await admin.goto('/payments');
			await expect(admin.getByTestId('payment-row').first()).toBeVisible();
			await expectReadableContrast(admin);
		});

		test('Products with the New product dialog open', async ({ admin, product }) => {
			await admin.goto('/products');
			await expect(admin.getByTestId('product-list')).toContainText(product.name);
			await admin.getByTestId('new-product-button').click();
			await expect(admin.getByTestId('dialog-new-product')).toBeVisible();
			await expectReadableContrast(admin);
		});

		test('Snap Choose', async ({ page, api, product }) => {
			const { token } = await newSnapPayment(api, product.sandboxKey);
			await page.goto(snapPath(token));
			await expect(page.getByTestId('snap-methods')).toBeVisible();
			await expectReadableContrast(page);
		});

		test('Snap VA', async ({ page, api, product }) => {
			const { token } = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
			await page.goto(snapPath(token));
			await expect(page.getByTestId('snap-va')).toBeVisible();
			await expectReadableContrast(page);
		});
	});
}
