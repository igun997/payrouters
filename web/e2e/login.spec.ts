// Login, route guard, sign-out and the session-expired dialog.
import type { Page } from '@playwright/test';
import { ADMIN_TOKEN, TOKEN_KEY } from './support/api';
import { expect, test } from './support/fixtures';
import { setStoredToken } from './support/flows';

async function storedToken(page: Page): Promise<string | null> {
	return page.evaluate((key) => localStorage.getItem(key), TOKEN_KEY);
}

test.describe('login', () => {
	test('shows the rejection message for a wrong token and stores nothing', async ({ page }) => {
		await page.goto('/login');
		await page.getByTestId('token-input').fill('not-the-admin-token');
		await page.getByRole('button', { name: 'Sign in' }).click();

		await expect(page.getByRole('alert')).toContainText('That token was not accepted');
		await expect(page).toHaveURL(/\/login/);
		expect(await storedToken(page)).toBeNull();
	});

	test('keeps sign-in disabled until a token is entered', async ({ page }) => {
		await page.goto('/login');
		const signIn = page.getByRole('button', { name: 'Sign in' });
		await expect(signIn).toBeDisabled();

		await page.getByTestId('token-input').fill('   ');
		await expect(signIn).toBeDisabled();

		await page.getByTestId('token-input').fill(ADMIN_TOKEN);
		await expect(signIn).toBeEnabled();
	});

	test('signs in with the correct token and lands on Overview', async ({ page }) => {
		await page.goto('/login');
		await page.getByTestId('token-input').fill(ADMIN_TOKEN);
		await page.getByRole('button', { name: 'Sign in' }).click();

		await expect(page).toHaveURL(/\/$/);
		await expect(page.getByTestId('shell-title')).toHaveText('Overview');
		expect(await storedToken(page)).toBe(ADMIN_TOKEN);
	});

	test('redirects a protected route to login and returns there after sign-in', async ({ page }) => {
		await page.goto('/products');
		await expect(page).toHaveURL(/\/login\?next=%2Fproducts$/);

		await page.getByTestId('token-input').fill(ADMIN_TOKEN);
		await page.getByRole('button', { name: 'Sign in' }).click();
		await expect(page).toHaveURL(/\/products$/);
		await expect(page.getByTestId('page-products')).toBeVisible();
	});

	test('sign out clears the token and returns to login', async ({ admin }, testInfo) => {
		await admin.goto('/');
		await expect(admin.getByTestId('shell-title')).toHaveText('Overview');

		if (testInfo.project.name === 'phone') {
			await admin.getByTestId('tab-more').click();
			await admin.getByRole('button', { name: 'Sign out' }).click();
		} else {
			await admin.getByRole('button', { name: 'Sign out' }).click();
		}

		await expect(admin).toHaveURL(/\/login/);
		await expect(admin.getByTestId('token-input')).toBeVisible();
		expect(await storedToken(admin)).toBeNull();

		// The admin fixture plants the token on every navigation, so check the guard on a bare page.
		const bare = await admin.context().newPage();
		await bare.goto('/payments');
		await expect(bare).toHaveURL(/\/login\?next=%2Fpayments$/);
	});

	test('a revoked stored token mid-session shows the session-expired dialog', async ({ admin }) => {
		await admin.goto('/payments');
		await expect(admin.getByTestId('page-payments')).toBeVisible();

		// The token stops being valid while the dashboard is open.
		await setStoredToken(admin, 'revoked-token-from-another-session');
		await admin.getByRole('link', { name: 'Cash-ins' }).click();

		const dialog = admin.getByTestId('dialog-session-expired');
		await expect(dialog).toBeVisible();
		await expect(dialog).toContainText('Your session ended');
		expect(await storedToken(admin)).toBeNull();

		await dialog.getByRole('button', { name: 'Sign in again' }).click();
		await expect(admin).toHaveURL(/\/login\?next=%2Fcash-ins$/);

		await admin.getByTestId('token-input').fill(ADMIN_TOKEN);
		await admin.getByRole('button', { name: 'Sign in' }).click();
		await expect(admin).toHaveURL(/\/cash-ins$/);
		await expect(admin.getByTestId('page-cash-ins')).toBeVisible();
	});

	test('signs in on a phone and shows the five-tab bar @phone', async ({ page }, testInfo) => {
		test.skip(testInfo.project.name !== 'phone', 'phone layout only');

		await page.goto('/login');
		await page.getByTestId('token-input').fill(ADMIN_TOKEN);
		await page.getByRole('button', { name: 'Sign in' }).click();

		await expect(page).toHaveURL(/\/$/);
		const bar = page.getByTestId('tab-bar');
		await expect(bar).toBeVisible();
		await expect(bar.locator('a, button')).toHaveCount(5);
		await expect(bar.getByRole('link', { name: 'Overview' })).toBeVisible();
		await expect(bar.getByTestId('tab-more')).toHaveText(/More/);
	});
});
