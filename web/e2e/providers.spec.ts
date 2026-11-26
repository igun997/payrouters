// Provider accounts: add (Midtrans sandbox, IDRX live-only, duplicate conflict), masked keys, a
// blank key on edit keeps the saved key, enable toggle, stubbed connection test, fee schedule
// changes reaching later cash-ins, and delete blocked by existing payments.
//
// Ordering: this file owns the Midtrans sandbox account in its database. Webhook specs reuse it
// through api.ensureProvider (their files sort after this one). Nothing here calls the real
// providers: "Test connection" is answered by a route stub, and payments use Midtrans snap
// without a method, which creates no charge at the provider.
import type { Locator, Page } from '@playwright/test';
import { MIDTRANS_SERVER_KEY, uid } from './support/api';
import { expect, test } from './support/fixtures';
import { MASK, grantClipboard, percentFee, readClipboard, rp } from './support/flows';

const midtransCard = (page: Page) =>
	page.locator('[data-testid="provider-row"][data-provider="midtrans"][data-mode="sandbox"]');
const mockCard = (page: Page) =>
	page.locator('[data-testid="provider-row"][data-provider="mock"][data-mode="sandbox"]');

async function chooseProvider(page: Page, dialog: Locator, label: string) {
	await dialog.getByRole('combobox', { name: 'Choose provider' }).click();
	await page.getByRole('option', { name: label, exact: true }).click();
}

test.describe('provider accounts', () => {
	test('adds a Midtrans sandbox account and never shows the raw key again', async ({ admin }) => {
		await grantClipboard(admin);
		await admin.goto('/providers');
		await admin.getByTestId('add-provider-button').click();

		const dialog = admin.getByTestId('dialog-add-provider');
		await chooseProvider(admin, dialog, 'Midtrans');
		await expect(dialog.getByRole('radio', { name: /^Sandbox/ })).toBeChecked();
		await dialog.getByLabel('Label', { exact: true }).fill('Midtrans sandbox e2e');
		await dialog.getByLabel('Server key', { exact: true }).fill(MIDTRANS_SERVER_KEY);
		await dialog.getByRole('button', { name: 'Save account' }).click();

		await expect(admin.getByText('Provider account created')).toBeVisible();
		const card = midtransCard(admin);
		await expect(card).toContainText('Midtrans sandbox e2e');
		await expect(card).toContainText(`${MASK}0001`);
		await expect(card).not.toContainText(MIDTRANS_SERVER_KEY);
		expect(await admin.content()).not.toContain(MIDTRANS_SERVER_KEY);

		// The inbound URL to paste into Midtrans is shown in full and copies.
		const hookUrl = (await card.locator('code').textContent())?.trim() ?? '';
		expect(hookUrl).toMatch(/\/hooks\//);
		await card.getByRole('button', { name: 'Copy', exact: true }).click();
		expect(await readClipboard(admin)).toBe(hookUrl);
	});

	test('a blank key on edit keeps the saved key, proven by a settled payment', async ({
		admin,
		api,
		product
	}) => {
		const midtrans = await api.ensureProvider('midtrans');
		const payment = await api.createPayment(product.sandboxKey, {
			order_id: uid('MT'),
			amount: 150000,
			provider: 'midtrans'
		});
		expect(payment.json.status).toBe('pending');

		await admin.goto('/providers');
		await midtransCard(admin)
			.getByRole('button', { name: /^Edit / })
			.click();
		const dialog = admin.getByTestId('dialog-edit-provider');
		await expect(dialog.getByText('Leave blank to keep the saved key')).toBeVisible();
		await dialog.getByLabel('Label', { exact: true }).fill('Midtrans sandbox e2e edited');
		await dialog.getByRole('button', { name: 'Save changes' }).click();

		await expect(admin.getByText('Provider account saved')).toBeVisible();
		await expect(midtransCard(admin)).toContainText('Midtrans sandbox e2e edited');

		// A notification signed with the original key must still verify and settle the payment.
		expect(await api.midtransNotification(midtrans, payment.json.id, 150000)).toBe(200);
		await admin.goto(`/payments/${payment.json.id}`);
		await expect(
			admin.getByTestId('page-payment-detail').getByTestId('status-badge').first()
		).toHaveAttribute('data-status', 'paid');
	});

	test('toggles an account off and back on', async ({ admin, api }) => {
		await api.ensureProvider('midtrans');
		await admin.goto('/providers');
		const card = midtransCard(admin);

		await card.getByRole('switch', { name: /^Enable / }).click();
		await expect(card).toContainText('Disabled');

		await card.getByRole('switch', { name: /^Enable / }).click();
		await expect(card).toContainText('Enabled');
	});

	test('Test connection shows the result banner (provider answer stubbed)', async ({
		admin,
		api
	}) => {
		await api.ensureProvider('midtrans');
		await admin.route('**/admin/providers/*/test', (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ ok: true, message: 'Authenticated against Midtrans sandbox' })
			})
		);
		await admin.goto('/providers');
		const card = midtransCard(admin);
		await card.getByRole('button', { name: /^Test connection for/ }).click();

		const result = card.getByTestId('test-result');
		await expect(result).toHaveAttribute('data-ok', 'true');
		await expect(result).toContainText('Authenticated against Midtrans sandbox');
	});

	test('IDRX locks the mode to Live and warns that there is no sandbox', async ({ admin }) => {
		await admin.goto('/providers');
		await admin.getByTestId('add-provider-button').click();
		const dialog = admin.getByTestId('dialog-add-provider');
		await chooseProvider(admin, dialog, 'IDRX');

		await expect(dialog.getByLabel('Mode (locked)')).toHaveValue('Live');
		await expect(dialog.getByTestId('provider-mode')).toHaveCount(0);
		await expect(dialog.getByText('No sandbox: every payment moves real money')).toBeVisible();
		await expect(dialog).toContainText('IDRX accounts are live only');

		await dialog.getByRole('button', { name: 'Cancel' }).click();
		await expect(dialog).toBeHidden();
	});

	test('a duplicate provider and mode shows the conflict error', async ({ admin, api }) => {
		await api.ensureProvider('midtrans');
		await admin.goto('/providers');
		await admin.getByTestId('add-provider-button').click();

		const dialog = admin.getByTestId('dialog-add-provider');
		await chooseProvider(admin, dialog, 'Midtrans');
		await dialog.getByLabel('Server key', { exact: true }).fill(MIDTRANS_SERVER_KEY);
		await dialog.getByRole('button', { name: 'Save account' }).click();

		await expect(dialog.getByRole('alert')).toContainText('already exists; edit it instead');
		await expect(dialog).toBeVisible();
	});

	test('a fee schedule edit changes the cash-in fee of a later payment', async ({
		admin,
		api,
		product
	}) => {
		await api.setMockFees({});
		await admin.goto('/providers');
		await mockCard(admin)
			.getByRole('button', { name: /^Edit / })
			.click();

		const dialog = admin.getByTestId('dialog-edit-provider');
		await dialog.getByRole('button', { name: 'Add method fee' }).click();
		await dialog.getByLabel('Percent', { exact: true }).fill('0.7');
		await dialog.getByRole('button', { name: 'Save changes' }).click();
		await expect(admin.getByText('Provider account saved')).toBeVisible();

		// A payment settled after the change carries the new schedule.
		const paid = await api.paidPayment(product.sandboxKey, 150000);
		const fee = percentFee(150000, 0.7);
		await admin.goto(`/payments/${paid.id}`);
		const ledger = admin.getByTestId('cash-in-ledger');
		await expect(ledger).toContainText(rp(fee));
		await expect(ledger).toContainText(rp(150000 - fee));

		await admin.goto('/cash-ins');
		await admin.getByRole('combobox', { name: 'Product' }).click();
		await admin.getByRole('option', { name: product.name, exact: true }).click();
		await expect(admin.getByTestId('cash-in-totals')).toContainText(rp(fee));
	});

	test('delete is blocked by existing payments and shows the In Use dialog', async ({
		admin,
		api,
		product
	}) => {
		await api.ensureProvider('midtrans');
		const payment = await api.createPayment(product.sandboxKey, {
			order_id: uid('MT-USE'),
			amount: 90000,
			provider: 'midtrans'
		});

		await admin.goto('/providers');
		await midtransCard(admin)
			.getByRole('button', { name: /^Edit / })
			.click();
		await admin
			.getByTestId('dialog-edit-provider')
			.getByRole('button', { name: 'Delete account' })
			.click();

		const confirm = admin.getByTestId('dialog-delete-provider');
		await confirm.getByRole('button', { name: 'Delete account' }).click();

		const inUse = admin.getByTestId('dialog-provider-in-use');
		await expect(inUse).toBeVisible();
		await expect(inUse).toContainText('Payments reference this account');
		await expect(inUse).toContainText(payment.json.order_id);

		await inUse.getByRole('button', { name: 'Cancel' }).click();
		await expect(inUse).toBeHidden();
		await expect(midtransCard(admin)).toBeVisible();
	});
});
