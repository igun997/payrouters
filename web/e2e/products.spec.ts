// Products: create (form validation, one-time webhook secret), sandbox keys against the product
// API, edit, rotate secret, revoke key and archive.
import type { Locator, Page } from '@playwright/test';
import { uid } from './support/api';
import { expect, test } from './support/fixtures';
import { grantClipboard, productCall, readClipboard } from './support/flows';

const productRow = (page: Page, name: string) =>
	page.getByTestId('product-row').filter({ hasText: name });

/** Opens the API keys disclosure of a product card (desktop opens the first product by default). */
async function expandKeys(row: Locator) {
	const toggle = row.getByRole('button', { name: /^API keys/ });
	if ((await toggle.getAttribute('aria-expanded')) === 'false') await toggle.click();
	await expect(toggle).toHaveAttribute('aria-expanded', 'true');
}

test.describe('products', () => {
	test('validates the new product form', async ({ admin }) => {
		await admin.goto('/products');
		await admin.getByTestId('new-product-button').click();
		const dialog = admin.getByRole('dialog', { name: 'New product' });

		await dialog.getByRole('button', { name: 'Create product' }).click();
		await expect(dialog.getByRole('alert')).toContainText('Enter a name for the product');

		await dialog.getByLabel('Name', { exact: true }).fill(uid('Shop'));
		await dialog.getByLabel('Webhook URL', { exact: true }).fill('http://hooks.example.com/x');
		await dialog.getByRole('button', { name: 'Create product' }).click();
		await expect(dialog.getByRole('alert')).toContainText('Webhook URL must start with https://');
	});

	test('creates a product and shows its webhook secret once', async ({ admin }) => {
		await grantClipboard(admin);
		const name = uid('Shop');

		await admin.goto('/products');
		await admin.getByTestId('new-product-button').click();
		const form = admin.getByRole('dialog', { name: 'New product' });
		await form.getByLabel('Name', { exact: true }).fill(name);
		await form.getByLabel('Webhook URL', { exact: true }).fill('https://example.com/payrouters');
		await form.getByRole('button', { name: 'Create product' }).click();

		const reveal = admin.getByRole('dialog', { name: 'Product created' });
		const secret = await reveal.getByRole('textbox', { name: 'Webhook secret' }).inputValue();
		expect(secret).toMatch(/^whsec_/);

		await reveal.getByRole('button', { name: 'Copy', exact: true }).click();
		await expect(admin.getByText('Copied to clipboard')).toBeVisible();
		expect(await readClipboard(admin)).toBe(secret);

		await reveal.getByRole('button', { name: 'I have copied it' }).click();
		await expect(reveal).toBeHidden();
		await expect(productRow(admin, name)).toBeVisible();

		// Closed for good: the secret is never displayed again, not even after a reload.
		await admin.reload();
		await expect(productRow(admin, name)).toBeVisible();
		expect(await admin.content()).not.toContain(secret);
	});

	test('creates a sandbox key the product API accepts, then revoking locks it out', async ({
		admin,
		request,
		product
	}) => {
		await admin.goto('/products');
		const row = productRow(admin, product.name);
		await expect(row).toBeVisible();
		await expandKeys(row);

		await row.getByTestId('create-api-key-button').click();
		const form = admin.getByRole('dialog', { name: 'New API key' });
		const keyName = uid('backend');
		await form.getByLabel('Key name').fill(keyName);
		await form.getByRole('button', { name: 'Create key' }).click();

		const reveal = admin.getByRole('dialog', { name: 'API key created' });
		const key = await reveal.getByRole('textbox', { name: 'API key' }).inputValue();
		expect(key).toMatch(/^prk_test_/);
		await reveal.getByRole('button', { name: 'Done' }).click();
		await expect(reveal).toBeHidden();

		// The key was shown once: the list only has its prefix now.
		expect(await admin.content()).not.toContain(key);
		await expect(row.getByTestId('api-key-row').filter({ hasText: keyName })).toHaveAttribute(
			'data-status',
			'active'
		);

		expect((await productCall(request, key, 'GET', '/v1/methods')).status).toBe(200);

		await row.getByRole('button', { name: `Revoke ${keyName}` }).click();
		const confirm = admin.getByRole('dialog', { name: 'Revoke API key?' });
		await confirm.getByRole('button', { name: 'Revoke key' }).click();
		await expect(admin.getByText('API key revoked')).toBeVisible();
		await expect(row.getByTestId('api-key-row').filter({ hasText: keyName })).toHaveAttribute(
			'data-status',
			'revoked'
		);

		expect((await productCall(request, key, 'GET', '/v1/methods')).status).toBe(401);
	});

	test('edits the webhook URL and rejects an http address', async ({ admin, product }) => {
		await admin.goto('/products');
		const row = productRow(admin, product.name);
		await row.getByRole('button', { name: `Edit ${product.name}` }).click();

		const dialog = admin.getByRole('dialog', { name: `Edit ${product.name}` });
		await dialog.getByLabel('Webhook URL', { exact: true }).fill('http://hooks.example.com/a');
		await dialog.getByRole('button', { name: 'Save', exact: true }).click();
		await expect(dialog.getByRole('alert')).toContainText('Webhook URL must start with https://');

		await dialog.getByLabel('Webhook URL', { exact: true }).fill('https://hooks.example.com/a');
		await dialog.getByRole('button', { name: 'Save', exact: true }).click();
		await expect(admin.getByText('Product saved')).toBeVisible();
		await expect(row).toContainText('https://hooks.example.com/a');
	});

	test('rotates the webhook secret only after confirmation', async ({ admin, product }) => {
		await admin.goto('/products');
		const row = productRow(admin, product.name);

		await row.getByRole('button', { name: 'Rotate secret' }).click();
		const confirm = admin.getByRole('dialog', { name: 'Rotate webhook secret?' });
		await confirm.getByRole('button', { name: 'Cancel' }).click();
		await expect(confirm).toBeHidden();
		await expect(admin.getByRole('dialog', { name: 'Webhook secret rotated' })).toHaveCount(0);

		await row.getByRole('button', { name: 'Rotate secret' }).click();
		await confirm.getByRole('button', { name: 'Rotate secret' }).click();

		const reveal = admin.getByRole('dialog', { name: 'Webhook secret rotated' });
		const next = await reveal.getByRole('textbox', { name: 'Webhook secret' }).inputValue();
		expect(next).toMatch(/^whsec_/);
		expect(next).not.toBe(product.webhookSecret);
		await reveal.getByRole('button', { name: 'I have copied it' }).click();
		expect(await admin.content()).not.toContain(next);
	});

	test('archiving removes the product row and its keys stop working', async ({
		admin,
		request,
		product
	}) => {
		await admin.goto('/products');
		const row = productRow(admin, product.name);
		await row.getByRole('button', { name: `More actions for ${product.name}` }).click();
		await admin.getByRole('menuitem', { name: 'Archive product' }).click();

		const confirm = admin.getByRole('dialog', { name: `Archive ${product.name}?` });
		await expect(confirm).toContainText('Revokes all of its API keys');
		await confirm.getByRole('button', { name: 'Archive product' }).click();

		await expect(admin.getByText(`Archived ${product.name}`)).toBeVisible();
		await expect(row).toHaveCount(0);
		expect((await productCall(request, product.sandboxKey, 'GET', '/v1/methods')).status).toBe(401);
	});

	test('creates a product from the phone sheet @phone', async ({ admin }, testInfo) => {
		test.skip(testInfo.project.name !== 'phone', 'phone layout only');
		const name = uid('Phone');

		await admin.goto('/products');
		await admin.getByTestId('new-product-button').click();
		const sheet = admin.getByRole('dialog', { name: 'New product' });
		await sheet.getByLabel('Name', { exact: true }).fill(name);
		await sheet.getByRole('button', { name: 'Create product' }).click();

		const reveal = admin.getByRole('dialog', { name: 'Product created' });
		await expect(reveal.getByRole('textbox', { name: 'Webhook secret' })).toHaveValue(/^whsec_/);
		await reveal.getByRole('button', { name: 'I have copied it' }).click();
		await expect(productRow(admin, name)).toBeVisible();
	});
});
