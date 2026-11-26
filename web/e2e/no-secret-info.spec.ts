import { ADMIN_TOKEN } from './support/api';
import { expect, test } from './support/fixtures';

/** The admin token (or any part of it) and configuration names never appear in rendered pages. */
const PAGES = [
	'/',
	'/payments',
	'/cash-ins',
	'/products',
	'/providers',
	'/webhooks',
	'/playground'
];
const FORBIDDEN = ['ADMIN_TOKEN', 'MASTER_KEY', 'DATABASE_URL', 'PUBLIC_BASE_URL', '.env'];

async function expectNoSecrets(page: import('@playwright/test').Page, where: string) {
	const { text, html } = await page.evaluate(() => ({
		text: document.body.innerText,
		html: document.documentElement.outerHTML
	}));
	for (const needle of [ADMIN_TOKEN, ADMIN_TOKEN.slice(-6), ...FORBIDDEN]) {
		expect(text, `${where}: visible text contains "${needle}"`).not.toContain(needle);
		expect(html, `${where}: DOM contains "${needle}"`).not.toContain(needle);
	}
}

test.describe('no secret or config information in the UI', () => {
	for (const path of PAGES) {
		test(`admin page ${path}`, async ({ admin }) => {
			await admin.goto(path);
			await expect(admin.getByTestId('page-title').first()).toBeAttached();
			await expectNoSecrets(admin, path);
		});
	}

	test('login page, before and after a wrong attempt', async ({ page }) => {
		await page.goto('/login');
		await expectNoSecrets(page, '/login');
		await page.getByTestId('token-input').fill(`${ADMIN_TOKEN.slice(0, -1)}x`);
		await page.getByTestId('sign-in').click();
		await expect(page.getByRole('alert')).toBeVisible();
		await expectNoSecrets(page, '/login after error');
	});

	test('More screen on a phone shows no token tail @phone', async ({ admin }) => {
		test.skip((admin.viewportSize()?.width ?? 0) >= 768, 'phone-only screen');
		await admin.goto('/');
		await admin.getByTestId('tab-more').click();
		await expect(admin.getByTestId('page-more')).toBeVisible();
		await expect(admin.getByTestId('page-more')).not.toContainText('Token');
		await expectNoSecrets(admin, 'more');
	});
});
