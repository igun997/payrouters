import { expect, hasHorizontalScroll, test } from './support/fixtures';
import { uid } from './support/api';
import {
	idr,
	newSnapPayment,
	patchSnapState,
	readClipboard,
	snapPath
} from './support/snap-helpers';

const METHOD_ORDER = ['qris', 'va_bca', 'va_bni', 'va_bri', 'va_mandiri', 'va_permata'] as const;

test.describe('Snap customer page', () => {
	test('Choose lists QRIS first, then the five bank rows, with the sandbox badge', async ({
		page,
		api,
		product
	}) => {
		const { token } = await newSnapPayment(api, product.sandboxKey);
		await page.goto(snapPath(token));

		const methods = page.getByTestId('snap-methods');
		await expect(methods.getByRole('heading', { name: 'Choose how to pay' })).toBeVisible();
		await expect(methods.getByText('Scan to pay')).toBeVisible();
		await expect(methods.getByText('Bank transfer')).toBeVisible();

		const rows = methods.locator('[data-testid^="snap-method-"]');
		await expect(rows).toHaveCount(METHOD_ORDER.length);
		for (const [index, code] of METHOD_ORDER.entries()) {
			await expect(rows.nth(index)).toHaveAttribute('data-testid', `snap-method-${code}`);
		}

		await expect(page.getByTestId('snap-amount')).toHaveText(idr(150000));
		await expect(page.getByText('Sandbox', { exact: true }).filter({ visible: true })).toHaveCount(
			1
		);
	});

	test('BCA shows the virtual account, amount, countdown and how to pay; copy buttons write the clipboard', async ({
		page,
		context,
		api,
		product
	}) => {
		await context.grantPermissions(['clipboard-read', 'clipboard-write']);
		const { token } = await newSnapPayment(api, product.sandboxKey);
		await page.goto(snapPath(token));

		await page.getByTestId('snap-method-va_bca').click();
		await expect(page.getByTestId('snap-va')).toBeVisible();

		// Mock VA numbers are "8" plus twelve digits; the groups are spans, so textContent is the bare digits.
		const account = page.getByTestId('snap-account-number');
		await expect(account).toHaveText(/^8\d{12}$/);
		const digits = ((await account.textContent()) ?? '').trim();
		await expect(page.getByText('PAYROUTERS MOCK')).toBeVisible();

		await expect(page.getByTestId('snap-amount')).toHaveText(idr(150000));
		await expect(page.getByTestId('snap-countdown')).toHaveText(/^\d{2}:\d{2}:\d{2}$/);
		await expect(page.getByTestId('snap-how-to-pay')).toBeVisible();
		await expect(page.getByRole('heading', { name: 'How to pay' })).toBeVisible();

		const copyAccount = page.getByTestId('snap-copy-account');
		await copyAccount.click();
		await expect(copyAccount).toHaveAccessibleName('Copied');
		await expect(page.getByText('Account number copied')).toBeVisible();
		expect(await readClipboard(page)).toBe(digits);

		const copyAmount = page.getByTestId('snap-copy-amount');
		await copyAmount.click();
		await expect(copyAmount).toHaveAccessibleName('Copied');
		expect(await readClipboard(page)).toBe('150000');
	});

	test('simulator Paid moves the open page to the paid state', async ({ page, api, product }) => {
		const { token } = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
		await page.goto(snapPath(token));

		await expect(page.getByTestId('snap-simulator')).toBeVisible();
		await page.getByTestId('snap-simulate-paid').click();

		const status = page.getByTestId('snap-status');
		await expect(status).toHaveAttribute('data-status', 'paid');
		await expect(status.getByRole('heading', { name: 'Payment received' })).toBeVisible();
		await expect(status).toContainText(idr(150000));
		await expect(page.getByTestId('snap-simulator')).toHaveCount(0);
	});

	test('simulator Expired shows the expired result', async ({ page, api, product }) => {
		const { token } = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
		await page.goto(snapPath(token));

		await page.getByTestId('snap-simulate-expired').click();
		const status = page.getByTestId('snap-status');
		await expect(status).toHaveAttribute('data-status', 'expired');
		await expect(status.getByRole('heading', { name: 'Payment expired' })).toBeVisible();
	});

	test('simulator Failed shows the failed result', async ({ page, api, product }) => {
		const { token } = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
		await page.goto(snapPath(token));

		await page.getByTestId('snap-simulate-failed').click();
		const status = page.getByTestId('snap-status');
		await expect(status).toHaveAttribute('data-status', 'failed');
		await expect(status.getByRole('heading', { name: 'Payment failed' })).toBeVisible();
		await expect(status).toContainText('No money was taken');
	});

	test('paid page with return_url counts down to the merchant; Cancel stops it', async ({
		page,
		api,
		product
	}) => {
		await page.route('https://merchant.example.test/**', (route) =>
			route.fulfill({ status: 200, contentType: 'text/html', body: '<h1>Merchant</h1>' })
		);
		const { token } = await newSnapPayment(api, product.sandboxKey, {
			method: 'va_bca',
			return_url: 'https://merchant.example.test/thanks'
		});
		await page.goto(snapPath(token));

		await page.getByTestId('snap-simulate-paid').click();
		await expect(page.getByTestId('snap-status')).toHaveAttribute('data-status', 'paid');

		const countdown = page
			.getByRole('status')
			.filter({ hasText: 'Returning to merchant.example.test' });
		await expect(countdown).toContainText(/Returning to merchant\.example\.test in \d seconds?/);
		const back = page.getByRole('button', { name: `Back to ${product.name}`, exact: true });
		await expect(back).toBeVisible();

		await page.getByRole('button', { name: 'Cancel', exact: true }).click();
		await expect(countdown).toHaveCount(0);
		await expect(back).toBeVisible();
		await expect(page).toHaveURL(new RegExp(`/snap/${token}$`));
	});

	test('QRIS shows a QR image with alt text and the Save QR button', async ({
		page,
		api,
		product
	}) => {
		const { token, orderId } = await newSnapPayment(api, product.sandboxKey, { method: 'qris' });
		await page.goto(snapPath(token));

		await expect(page.getByTestId('snap-qris')).toBeVisible();
		await expect(page.getByRole('img', { name: `QRIS code for order ${orderId}` })).toBeVisible();
		const save = page.getByRole('link', { name: 'Save QR image' });
		await expect(save).toHaveAttribute('href', /^data:image\/png;base64,/);
		await expect(save).toHaveAttribute('download', `qris-${orderId}.png`);
	});

	test('Mandiri chosen through the API shows biller code and bill key, not an account number', async ({
		page,
		api,
		product
	}) => {
		const { token, json } = await newSnapPayment(api, product.sandboxKey, {
			method: 'va_mandiri'
		});
		const bill = json.instructions as { biller_code: string | null; bill_key: string | null };
		expect(bill.biller_code).toBe('70012');
		expect(bill.bill_key).toMatch(/^\d{12}$/);

		await page.goto(snapPath(token));
		await expect(page.getByTestId('snap-mandiri-bill')).toBeVisible();
		await expect(page.getByTestId('snap-biller-code')).toHaveText('70012');
		await expect(page.getByTestId('snap-bill-key')).toHaveText(bill.bill_key ?? '');
		await expect(page.getByTestId('snap-account-number')).toHaveCount(0);
	});

	test('provider checkout (redirect) shows the hop panel and moves on to the provider', async ({
		page,
		api,
		product
	}) => {
		const providerUrl = 'https://provider.example.test/checkout/e2e';
		await page.route('https://provider.example.test/**', (route) =>
			route.fulfill({ status: 200, contentType: 'text/html', body: '<h1>Provider</h1>' })
		);
		const { token } = await newSnapPayment(api, product.sandboxKey);
		await patchSnapState(page, token, (state) => {
			state.payment.provider = 'midtrans';
			state.payment.instructions = { type: 'redirect', url: providerUrl };
			state.methods = [];
		});
		await page.goto(snapPath(token));

		const panel = page.getByTestId('snap-redirect');
		await expect(
			panel.getByRole('heading', { name: 'You will finish this payment on Midtrans' })
		).toBeVisible();
		await expect(panel.getByRole('button', { name: 'Continue to Midtrans' })).toBeVisible();
		await expect(page).toHaveURL(providerUrl, { timeout: 7000 });
	});

	test('provider refuses the start: row error and toast, then retry works', async ({
		page,
		api,
		product
	}) => {
		const { token } = await newSnapPayment(api, product.sandboxKey);
		const message = `Bank BCA is not available right now ${uid('err')}`;
		let calls = 0;
		await page.route(`**/snap/api/${token}/pay`, async (route) => {
			calls += 1;
			if (calls === 1) {
				await route.fulfill({
					status: 502,
					contentType: 'application/json',
					json: { error: { code: 'provider_error', message, details: null } }
				});
				return;
			}
			await route.continue();
		});
		await page.goto(snapPath(token));

		const bca = page.getByTestId('snap-method-va_bca');
		await bca.click();
		await expect(bca).toHaveAttribute('data-state', 'error');
		await expect(bca).toContainText('Could not start this payment. Try again.');
		await expect(page.getByText(message)).toBeVisible();

		await bca.click();
		await expect(page.getByTestId('snap-va')).toBeVisible();
		expect(calls).toBe(2);
	});

	test('a payable amount above the amount shows the breakdown and the exact total', async ({
		page,
		api,
		product
	}) => {
		const { token } = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
		await patchSnapState(page, token, (state) => {
			state.payment.payable_amount = 153000;
		});
		await page.goto(snapPath(token));

		await expect(page.getByTestId('snap-va')).toBeVisible();
		await expect(page.getByTestId('snap-amount')).toHaveText(idr(153000));
		await expect(page.getByText('Breakdown', { exact: true })).toBeVisible();
		await expect(page.getByText('IDRX to mint')).toBeVisible();
		await expect(page.locator('dd').filter({ hasText: idr(150000) })).toHaveCount(1);
		await expect(page.getByText('VA fee')).toBeVisible();
		await expect(page.locator('dd').filter({ hasText: idr(3000) })).toHaveCount(1);
		await expect(page.locator('dt', { hasText: 'Total to pay' })).toBeVisible();
		await expect(page.locator('dd').filter({ hasText: idr(153000) })).toHaveCount(1);
		await expect(page.getByText(/Pay exactly Rp\s153\.000/)).toBeVisible();
		await expect(page.getByTestId('snap-how-to-pay')).toHaveCount(0);
	});

	test('unknown token shows the Not Found state', async ({ page }) => {
		await page.goto(snapPath(uid('missing')));
		const status = page.getByTestId('snap-status');
		await expect(status).toHaveAttribute('data-status', 'not_found');
		await expect(page.getByText('This payment link is not valid')).toBeVisible();
	});

	test('payment settled through the API flips the open page to paid within 6 s', async ({
		page,
		api,
		product
	}) => {
		const { token } = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
		await page.goto(snapPath(token));
		await expect(page.getByTestId('snap-va')).toBeVisible();

		expect(await api.simulate(token, 'paid')).toBe(200);
		await expect(page.getByTestId('snap-status')).toHaveAttribute('data-status', 'paid', {
			timeout: 6000
		});
	});
});

test.describe('Snap phone flow @phone 390', () => {
	test.use({ viewport: { width: 390, height: 844 } });

	test('full flow at 390: Choose, BCA, Paid @phone', async ({ page, api, product }) => {
		const { token } = await newSnapPayment(api, product.sandboxKey);
		await page.goto(snapPath(token));

		await page.getByTestId('snap-method-va_bca').click();
		await expect(page.getByTestId('snap-account-number')).toBeVisible();
		await expect(page.getByTestId('snap-amount')).toHaveText(idr(150000));
		expect(await hasHorizontalScroll(page)).toBe(false);

		await page.getByTestId('snap-simulate-paid').click();
		const status = page.getByTestId('snap-status');
		await expect(status).toHaveAttribute('data-status', 'paid');
		await expect(status.getByRole('heading', { name: 'Payment received' })).toBeVisible();
		expect(await hasHorizontalScroll(page)).toBe(false);
	});
});

test.describe('Snap phone flow @phone 320', () => {
	test.use({ viewport: { width: 320, height: 568 } });

	test('full flow at 320: Choose, BCA, Paid @phone', async ({ page, api, product }) => {
		const { token } = await newSnapPayment(api, product.sandboxKey);
		await page.goto(snapPath(token));

		await page.getByTestId('snap-method-va_bca').click();
		await expect(page.getByTestId('snap-account-number')).toBeVisible();
		await expect(page.getByTestId('snap-amount')).toHaveText(idr(150000));
		expect(await hasHorizontalScroll(page)).toBe(false);

		await page.getByTestId('snap-simulate-paid').click();
		const status = page.getByTestId('snap-status');
		await expect(status).toHaveAttribute('data-status', 'paid');
		await expect(status.getByRole('heading', { name: 'Payment received' })).toBeVisible();
		expect(await hasHorizontalScroll(page)).toBe(false);
	});
});
