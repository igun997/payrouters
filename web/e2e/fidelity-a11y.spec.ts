// Accessibility beyond axe: document language and per-route titles, accessible names for every visible
// control (icon-only buttons included), visible keyboard focus in both themes, reduced motion, and form
// errors announced and linked to their control.
import type { Page } from '@playwright/test';
import { expect, test } from './support/fixtures';
import { newSnapPayment, snapPath } from './support/snap-helpers';

/** Visible controls with no accessible name, as an HTML snippet each. */
async function unnamedControls(page: Page): Promise<string[]> {
	return page.evaluate(() => {
		const out: string[] = [];
		const controls = document.querySelectorAll<HTMLElement>(
			'button, a[href], [role="button"], [role="switch"], [role="checkbox"], [role="radio"], [role="combobox"], input:not([type="hidden"]), select, textarea'
		);
		for (const el of controls) {
			if (el.getClientRects().length === 0 || el.closest('[aria-hidden="true"]')) continue;
			const labelledBy = (el.getAttribute('aria-labelledby') ?? '')
				.split(/\s+/)
				.map((id) => document.getElementById(id)?.textContent?.trim() ?? '')
				.join(' ')
				.trim();
			const name =
				el.getAttribute('aria-label')?.trim() ||
				labelledBy ||
				(el as HTMLInputElement).labels?.[0]?.textContent?.trim() ||
				el.textContent?.trim() ||
				el.querySelector('img[alt]')?.getAttribute('alt')?.trim() ||
				el.getAttribute('title')?.trim() ||
				'';
			if (!name) out.push(el.outerHTML.slice(0, 140));
		}
		return out;
	});
}

/** Tabs through the first `count` stops and returns the ones without a visible focus indicator. */
async function focusWithoutIndicator(page: Page, count: number): Promise<string[]> {
	const missing: string[] = [];
	for (let i = 0; i < count; i++) {
		await page.keyboard.press('Tab');
		const report = await page.evaluate(() => {
			const el = document.activeElement as HTMLElement | null;
			if (!el || el === document.body) return null;
			const s = getComputedStyle(el);
			const outlined = s.outlineStyle !== 'none' && parseFloat(s.outlineWidth) > 0;
			const shadowed = s.boxShadow !== 'none' && s.boxShadow !== '';
			return outlined || shadowed ? null : el.outerHTML.slice(0, 120);
		});
		if (report) missing.push(report);
	}
	return missing;
}

test.describe('document language and titles', () => {
	test('html lang is English on every route', async ({ page }) => {
		await page.goto('/login');
		await expect(page.locator('html')).toHaveAttribute('lang', 'en');
	});

	test('each admin route publishes its own title', async ({ admin, api, product }) => {
		const paid = await api.paidPayment(product.sandboxKey, 150000);
		const routes: [string, RegExp][] = [
			['/', /^Overview \| PayRouters$/],
			['/payments', /^Payments \| PayRouters$/],
			[`/payments/${paid.id}`, /^Payment detail \| PayRouters$/],
			['/cash-ins', /^Cash-in recap \| PayRouters$/],
			['/products', /^Products \| PayRouters$/],
			['/providers', /^Provider accounts \| PayRouters$/],
			['/webhooks', /^Webhooks \| PayRouters$/],
			['/playground', /^Sandbox playground \| PayRouters$/]
		];
		for (const [path, title] of routes) {
			await admin.goto(path);
			await expect(admin, path).toHaveTitle(title);
		}
	});

	test('the Snap page titles itself with the order', async ({ page, api, product }) => {
		const { token, orderId } = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
		await page.goto(snapPath(token));
		await expect(page.getByTestId('snap-va')).toBeVisible();
		await expect(page).toHaveTitle(`Pay ${orderId} | PayRouters`);
	});
});

test.describe('accessible names', () => {
	test.use({ viewport: { width: 1366, height: 768 } });

	test('every visible control on the admin pages and dialogs has a name', async ({
		admin,
		api,
		product
	}) => {
		const paid = await api.paidPayment(product.sandboxKey, 150000);
		const pages = [
			'/',
			'/payments',
			`/payments/${paid.id}`,
			'/cash-ins',
			'/products',
			'/providers',
			'/webhooks',
			'/playground'
		];
		const failures: string[] = [];
		for (const path of pages) {
			await admin.goto(path);
			await admin.waitForLoadState('networkidle');
			for (const bad of await unnamedControls(admin)) failures.push(`${path}: ${bad}`);
		}

		await admin.goto('/products');
		await expect(admin.getByTestId('product-list')).toBeVisible();
		await admin.getByTestId('new-product-button').click();
		await expect(admin.getByTestId('dialog-new-product')).toBeVisible();
		for (const bad of await unnamedControls(admin)) failures.push(`dialog-new-product: ${bad}`);
		await admin.keyboard.press('Escape');

		await admin.goto('/providers');
		await expect(admin.getByTestId('provider-list')).toBeVisible();
		await admin.getByTestId('add-provider-button').click();
		await expect(admin.getByTestId('dialog-add-provider')).toBeVisible();
		for (const bad of await unnamedControls(admin)) failures.push(`dialog-add-provider: ${bad}`);

		expect(failures, failures.join('\n')).toEqual([]);
	});
});

test.describe('keyboard focus', () => {
	for (const scheme of ['light', 'dark'] as const) {
		test(`focus is visible on the first controls of Overview in ${scheme} mode`, async ({
			admin,
			api,
			product
		}) => {
			await api.paidPayment(product.sandboxKey, 150000);
			await admin.emulateMedia({ colorScheme: scheme });
			await admin.goto('/');
			await expect(admin.getByTestId('kpi-conversion')).toBeVisible();
			await admin.locator('body').click({ position: { x: 1, y: 1 } });
			const missing = await focusWithoutIndicator(admin, 14);
			expect(missing, missing.join('\n')).toEqual([]);
		});
	}
});

test.describe('reduced motion', () => {
	test.use({ reducedMotion: 'reduce', viewport: { width: 390, height: 844 } });

	test('the Snap loading skeleton does not pulse when reduced motion is requested', async ({
		page,
		api,
		product
	}) => {
		const { token } = await newSnapPayment(api, product.sandboxKey);
		// The state request never resolves, so the loading skeleton stays on screen.
		await page.route(`**/snap/api/${token}`, () => {});
		await page.goto(snapPath(token));
		const loading = page.getByTestId('snap-loading');
		await expect(loading).toBeVisible();
		const animated = await loading
			.locator('[data-testid="skeleton-block"]')
			.evaluateAll(
				(els) => els.filter((el) => getComputedStyle(el).animationName !== 'none').length
			);
		expect(animated).toBe(0);
	});

	test('the pending status dot stops pinging when reduced motion is requested', async ({
		page,
		api,
		product
	}) => {
		const { token } = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
		await page.goto(snapPath(token));
		await expect(page.getByTestId('snap-status')).toBeVisible();
		const pinging = await page
			.getByTestId('snap-status')
			.locator('.animate-ping')
			.evaluate((el) => getComputedStyle(el).display);
		expect(pinging).toBe('none');
	});
});

test.describe('form errors', () => {
	test.use({ viewport: { width: 1366, height: 768 } });

	test('a missing product name is announced with role alert and linked to its input', async ({
		admin
	}) => {
		await admin.goto('/products');
		await admin.getByTestId('new-product-button').click();
		const dialog = admin.getByTestId('dialog-new-product');
		await expect(dialog).toBeVisible();

		await dialog.getByRole('button', { name: 'Create product', exact: true }).click();
		const name = dialog.getByLabel('Name', { exact: true });
		await expect(name).toHaveAttribute('aria-invalid', 'true');
		const alert = dialog.getByRole('alert').filter({ hasText: 'Enter a name for the product' });
		await expect(alert).toHaveCount(1);
		const alertId = await alert.getAttribute('id');
		expect(alertId).toBeTruthy();
		await expect(name).toHaveAttribute('aria-describedby', new RegExp(`(^|\\s)${alertId}(\\s|$)`));
	});
});
