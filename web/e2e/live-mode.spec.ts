// Sandbox / Live switch on desktop: cancelling keeps Sandbox, confirming shows the persistent Live
// banner and the live scope, switching back removes the banner.
import type { Page } from '@playwright/test';
import { expect, test } from './support/fixtures';
import { switchModeDesktop } from './support/flows';

const LIVE_BANNER = 'Live mode: payments move real money';

function modeRadio(page: Page, mode: 'Sandbox' | 'Live') {
	return page.getByTestId('mode-toggle').getByRole('radio', { name: mode, exact: true });
}

test.describe('live mode', () => {
	test('cancelling the switch to Live keeps Sandbox', async ({ admin }) => {
		await admin.goto('/');
		await modeRadio(admin, 'Live').click();

		const dialog = admin.getByTestId('dialog-live-mode');
		await expect(dialog).toContainText('Switch to Live mode?');
		await dialog.getByRole('button', { name: 'Stay in Sandbox' }).click();

		await expect(dialog).toBeHidden();
		await expect(modeRadio(admin, 'Sandbox')).toHaveAttribute('aria-checked', 'true');
		await expect(admin.getByTestId('live-banner')).toHaveCount(0);
	});

	test('confirming Live shows a persistent banner and the live scope', async ({ admin }) => {
		await admin.goto('/playground');
		await switchModeDesktop(admin, 'live');

		const banner = admin.getByTestId('live-banner');
		await expect(banner).toContainText(LIVE_BANNER);
		await expect(admin.getByTestId('shell-title')).toHaveText('Live playground');
		await expect(admin.getByTestId('mode-readout')).toHaveAttribute('data-mode', 'live');

		await admin.getByRole('link', { name: 'Payments' }).click();
		await expect(admin.getByTestId('page-payments')).toBeVisible();
		await expect(banner).toContainText(LIVE_BANNER);
		await expect(admin.getByText('No payments in live yet')).toBeVisible();

		await admin.reload();
		await expect(banner).toContainText(LIVE_BANNER);
		await expect(modeRadio(admin, 'Live')).toHaveAttribute('aria-checked', 'true');
	});

	test('switching back to Sandbox removes the banner', async ({ admin }) => {
		await admin.goto('/');
		await switchModeDesktop(admin, 'live');
		await expect(admin.getByTestId('live-banner')).toBeVisible();

		await switchModeDesktop(admin, 'sandbox');
		await expect(admin.getByTestId('live-banner')).toHaveCount(0);

		await admin.reload();
		await expect(admin.getByTestId('live-banner')).toHaveCount(0);
		await expect(modeRadio(admin, 'Sandbox')).toHaveAttribute('aria-checked', 'true');
	});
});
