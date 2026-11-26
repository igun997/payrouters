// Phone screens (390 and 320): payment detail with a sticky action bar above the tab bar, webhook
// cards that expand, the Payments filter sheet with the date range, the status chip fade, and the
// cash-in filter chips. Phone-only: every test skips on the desktop project.
import type { Locator, Page } from '@playwright/test';
import { uid } from './support/api';
import { expect, hasHorizontalScroll, test, wrappedAmounts } from './support/fixtures';
import { percentFee, rp } from './support/flows';

/** Vertical scroll of the admin content column (the `main` element scrolls, not the window). */
async function scrollMain(page: Page, top: 'end' | number): Promise<void> {
	await page.locator('main').evaluate((el, target) => {
		el.scrollTop = target === 'end' ? el.scrollHeight : target;
	}, top);
}

async function box(locator: Locator) {
	const b = await locator.boundingBox();
	if (!b) throw new Error('element has no bounding box');
	return b;
}

for (const width of [390, 320] as const) {
	test.describe(`phone screens ${width}`, () => {
		test.use({ viewport: { width, height: width === 320 ? 568 : 844 } });

		// Playwright needs the destructured fixtures argument even though none are used.
		// eslint-disable-next-line no-empty-pattern
		test.beforeEach(({}, testInfo) => {
			test.skip(testInfo.project.name !== 'phone', 'phone layouts only');
		});

		test(`payment detail: compact header, ledger, details, activity and actions @phone`, async ({
			admin,
			api,
			product
		}) => {
			await api.setMockFees({ '*': { fixed: 0, percent: 0.7 } });
			const paid = await api.paidPayment(product.sandboxKey, 150000);
			const fee = percentFee(150000, 0.7);

			await admin.goto(`/payments/${paid.id}`);
			const orderId = admin.getByTestId('payment-order-id');
			await expect(orderId).toHaveText(paid.orderId);
			expect(await orderId.evaluate((el) => getComputedStyle(el).fontFamily)).toMatch(/mono/i);
			await expect(admin.getByRole('link', { name: 'Back to payments' })).toBeVisible();
			await expect(
				admin.getByTestId('page-payment-detail').getByTestId('status-badge').first()
			).toHaveAttribute('data-status', 'paid');

			const summary = admin.getByTestId('payment-summary');
			const ledger = admin.getByTestId('cash-in-ledger');
			const details = admin.getByTestId('payment-details');
			const activity = admin.getByTestId('activity-timeline');
			await expect(summary).toContainText(rp(150000));
			await expect(ledger).toContainText(rp(fee));
			await expect(ledger).toContainText(rp(150000 - fee));
			await expect(details).toContainText('Checkout');
			await expect(details).toContainText('Snap');
			await expect(activity).toContainText('Payment created');

			// Order as designed: amount, ledger, details, activity.
			const ys = [];
			for (const card of [summary, ledger, details, activity]) ys.push((await box(card)).y);
			expect(ys).toEqual([...ys].sort((a, b) => a - b));

			const bar = admin.getByTestId('payment-action-bar');
			await expect(bar.getByRole('button', { name: 'Sync with provider' })).toBeVisible();
			await expect(bar.getByRole('button', { name: 'Open Snap page' })).toBeVisible();
			expect(await bar.evaluate((el) => getComputedStyle(el).position)).toBe('sticky');

			expect(await wrappedAmounts(admin)).toEqual([]);
			expect(await hasHorizontalScroll(admin)).toBe(false);
		});

		test(`the action bar stays above the tab bar and clears the last card @phone`, async ({
			admin,
			api,
			product
		}) => {
			const paid = await api.paidPayment(product.sandboxKey, 150000);
			await admin.goto(`/payments/${paid.id}`);
			const bar = admin.getByTestId('payment-action-bar');
			const tabBar = admin.getByTestId('tab-bar');
			await expect(bar).toBeVisible();
			const tabTop = (await box(tabBar)).y;

			// Mid-page the bar is pinned to one viewport position wherever the content is scrolled.
			await scrollMain(admin, 150);
			const pinnedA = await box(bar);
			await scrollMain(admin, 300);
			const pinnedB = await box(bar);
			expect(pinnedB.y).toBeCloseTo(pinnedA.y, 0);
			expect(pinnedB.y + pinnedB.height).toBeLessThanOrEqual(tabTop + 1);

			// At the end of the page the last card is fully above the bar, and the bar above the tab bar.
			await scrollMain(admin, 'end');
			const last = await box(admin.getByTestId('payment-metadata'));
			const barBox = await box(bar);
			expect(last.y + last.height).toBeLessThanOrEqual(barBox.y + 1);
			expect(barBox.y + barBox.height).toBeLessThanOrEqual(tabTop + 1);
		});

		test(`a pending payment without a method shows the simulator hint with paid disabled @phone`, async ({
			admin,
			api,
			product
		}) => {
			const pending = await api.createPayment(product.sandboxKey, {
				order_id: uid('HINT'),
				amount: 40000,
				provider: 'mock'
			});
			await admin.goto(`/payments/${pending.json.id}`);
			const simulator = admin.getByTestId('simulator');
			await expect(simulator).toContainText('The customer has not chosen a method yet');
			await expect(simulator.getByRole('button', { name: 'Simulate paid' })).toBeDisabled();
			await expect(simulator.getByRole('button', { name: 'Simulate expired' })).toBeEnabled();
		});

		test(`the sync icon in the action bar runs the sync @phone`, async ({
			admin,
			api,
			product
		}) => {
			const paid = await api.paidPayment(product.sandboxKey, 80000);
			await admin.goto(`/payments/${paid.id}`);
			await admin
				.getByTestId('payment-action-bar')
				.getByRole('button', { name: 'Sync with provider' })
				.click();
			await expect(admin.getByText('Payment synced with provider')).toBeVisible();
		});

		test(`an inbound webhook card leads with outcome and time, and expands to JSON lines @phone`, async ({
			admin,
			api,
			product
		}) => {
			const midtrans = await api.ensureProvider('midtrans');
			const payment = await api.createPayment(product.sandboxKey, {
				order_id: uid('PHONE-HOOK'),
				amount: 150000,
				provider: 'midtrans'
			});
			expect(await api.midtransNotification(midtrans, payment.json.id, 150000)).toBe(200);

			await admin.goto('/webhooks');
			const row = admin.getByTestId('webhook-event-row').filter({
				has: admin.locator(`a[href$="/payments/${payment.json.id}"]`)
			});
			await expect(row).toHaveCount(1);
			await expect(row).toHaveAttribute('data-verified', 'true');

			// Header first: the outcome pill and the time sit in the first block, above the payment link.
			const head = row.locator(':scope > div').first();
			await expect(head.getByTestId('outcome-pill')).toHaveAttribute('data-outcome', 'applied');
			await expect(head).toContainText(/\d{1,2} \w{3} 2026/);
			expect((await box(head)).y).toBeLessThan(
				(await box(row.getByTestId('webhook-payment-link'))).y
			);

			await expect(row.getByTestId('webhook-event-detail')).toHaveCount(0);
			await row.getByRole('button', { name: 'Show details' }).click();
			const detail = row.getByTestId('webhook-event-detail');
			await expect(detail).toBeVisible();
			await expect(detail).toContainText('Signature');
			await expect(detail).toContainText('Verified');

			const blocks = detail.getByTestId('json-block');
			await expect(blocks).toHaveCount(2);
			for (let i = 0; i < 2; i++) {
				const block = blocks.nth(i);
				expect(await block.evaluate((el) => el.tagName)).toBe('PRE');
				expect(await block.evaluate((el) => getComputedStyle(el).fontFamily)).toMatch(/mono/i);
				const text = await block.innerText();
				expect(text.split('\n').length, 'one JSON element per line').toBeGreaterThan(2);
			}
			await expect(blocks.nth(1)).toContainText(payment.json.id);

			await row.getByRole('button', { name: 'Hide details' }).click();
			await expect(detail).toBeHidden();
			expect(await hasHorizontalScroll(admin)).toBe(false);
		});

		test(`an outbound delivery card shows status first, with Retry and details together @phone`, async ({
			admin,
			api
		}) => {
			const product = await api.createProduct(uid('PhoneHook'), 'http://127.0.0.1:9/none');
			await api.paidPayment(product.sandboxKey, 60000);

			await admin.goto('/webhooks');
			await admin.getByRole('tab', { name: 'Outbound deliveries' }).click();
			const row = admin.getByTestId('delivery-row').filter({ hasText: product.name });
			await expect(row).toHaveCount(1);
			await expect(
				row.locator(':scope > div').first().getByTestId('delivery-status')
			).toHaveAttribute('data-status', 'pending');
			await expect(row).toContainText('attempts');
			await expect(row.getByTestId('retry-delivery')).toBeEnabled();

			await expect(row.getByTestId('delivery-detail')).toHaveCount(0);
			await row.getByRole('button', { name: 'Show details' }).click();
			const detail = row.getByTestId('delivery-detail');
			await expect(detail).toContainText('Event ID');
			await expect(detail.getByTestId('json-block')).toBeVisible();
			await row.getByRole('button', { name: 'Hide details' }).click();
			await expect(detail).toBeHidden();
		});

		test(`the payments search placeholder fits and the filter sheet opens the date range @phone`, async ({
			admin
		}) => {
			await admin.goto('/payments');
			const search = admin.getByRole('textbox', { name: 'Search payments' });
			await expect(search).toHaveAttribute('placeholder', 'Search order or ref');
			const fits = await search.evaluate((el: HTMLInputElement) => {
				const style = getComputedStyle(el);
				// Lay the placeholder out in a hidden span with the input's font, then compare its
				// width with the room the input leaves for text (its padding is not text).
				const probe = document.createElement('span');
				probe.textContent = el.placeholder;
				probe.style.cssText = `position:absolute;visibility:hidden;white-space:nowrap;font-family:${style.fontFamily};font-size:${style.fontSize};font-weight:${style.fontWeight}`;
				document.body.appendChild(probe);
				const textWidth = probe.getBoundingClientRect().width;
				probe.remove();
				// clientWidth is not the box for an input; use the border box minus borders and padding.
				const inner =
					el.getBoundingClientRect().width -
					parseFloat(style.borderLeftWidth) -
					parseFloat(style.borderRightWidth) -
					parseFloat(style.paddingLeft) -
					parseFloat(style.paddingRight);
				return textWidth <= inner;
			});
			expect(fits, 'placeholder fits without truncation').toBe(true);

			await admin.getByRole('button', { name: 'Open filters' }).click();
			const filters = admin.getByTestId('sheet-payments-filter');
			await expect(filters).toBeVisible();
			const rangeRow = filters.getByTestId('filter-date-range');
			const before = await rangeRow.textContent();

			await rangeRow.click();
			const range = admin.getByTestId('sheet-date-range');
			await expect(range).toBeVisible();
			await expect(filters).toBeHidden();
			await range.getByRole('button', { name: '7d', exact: true }).click();
			await range.getByRole('button', { name: 'Apply range' }).click();

			await expect(filters).toBeVisible();
			await expect(range).toBeHidden();
			await expect(filters.getByTestId('filter-date-range')).not.toHaveText(before ?? '');

			await filters.getByRole('button', { name: 'Apply filters' }).click();
			await expect(filters).toBeHidden();
			expect(await hasHorizontalScroll(admin)).toBe(false);
		});

		test(`the status chips show a fade until they are scrolled to the end @phone`, async ({
			admin
		}) => {
			await admin.goto('/payments');
			const chips = admin.getByRole('group', { name: 'Status' });
			const fade = admin.getByTestId('status-chips-fade');
			await expect(chips.getByRole('button', { name: /Failed/ })).toBeAttached();
			await expect(fade).toBeVisible();

			await chips.evaluate((el) => (el.scrollLeft = el.scrollWidth));
			await expect(fade).toBeHidden();

			await chips.getByRole('button', { name: /Paid/ }).click();
			await expect(chips.getByRole('button', { name: /Paid/ })).toHaveAttribute(
				'aria-pressed',
				'true'
			);
		});

		test(`cash-in filters become removable chips and the totals lead with net received @phone`, async ({
			admin,
			api,
			product
		}) => {
			await api.paidPayment(product.sandboxKey, 100000);
			await admin.goto('/cash-ins');
			const totals = admin.getByTestId('cash-in-totals');
			await expect(totals).toBeVisible();

			// The net item is first on screen and spans the strip, though it is not first in the DOM.
			const net = totals.locator('div').filter({ hasText: 'Net received' });
			const gross = totals.locator('div').filter({ hasText: 'Gross' });
			await expect(net).toHaveCount(1);
			const netBox = await box(net);
			const grossBox = await box(gross);
			const stripBox = await box(totals);
			expect(netBox.y).toBeLessThan(grossBox.y);
			expect(netBox.width).toBeGreaterThan(stripBox.width * 0.8);

			// No desktop selects on phones: the filters live in a sheet.
			await expect(admin.getByTestId('cash-in-filters').getByRole('combobox')).toHaveCount(0);
			await admin.getByTestId('open-cash-in-filters').click();
			const sheet = admin.getByTestId('sheet-cash-in-filters');
			await sheet.getByRole('combobox', { name: 'Product' }).click();
			await admin.getByRole('option', { name: product.name, exact: true }).click();
			await sheet.getByRole('button', { name: 'Apply filters' }).click();
			await expect(sheet).toBeHidden();

			const chip = admin.getByTestId('filter-chip').filter({ hasText: product.name });
			await expect(chip).toHaveAttribute('data-filter', 'product');
			await expect(admin.getByTestId('cash-in-row')).toHaveCount(1);
			await expect(admin.getByTestId('open-cash-in-filters')).toContainText('(1)');

			await chip.click();
			await expect(admin.getByTestId('filter-chip')).toHaveCount(0);
			await expect(admin.getByTestId('open-cash-in-filters')).not.toContainText('(1)');
			expect(await wrappedAmounts(admin)).toEqual([]);
			expect(await hasHorizontalScroll(admin)).toBe(false);
		});

		test(`the cash-in export sits in the list header on phones @phone`, async ({
			admin,
			api,
			product
		}) => {
			await api.paidPayment(product.sandboxKey, 30000);
			await admin.goto('/cash-ins');
			const exportButton = admin.getByTestId('export-csv');
			await expect(exportButton).toHaveText(/Export CSV/);
			await expect(admin.getByRole('heading', { name: 'Settled payments' })).toBeVisible();
			expect((await box(exportButton)).y).toBeLessThan(
				(await box(admin.getByTestId('cash-in-list'))).y
			);

			const [download] = await Promise.all([admin.waitForEvent('download'), exportButton.click()]);
			expect(download.suggestedFilename()).toMatch(/^payrouters-cash-ins-sandbox-.*\.csv$/);
		});

		test(`overview conversion keeps the paid of closed line on phones @phone`, async ({
			admin,
			api,
			product
		}) => {
			await api.paidPayment(product.sandboxKey, 25000);
			await admin.goto('/');
			await expect(admin.getByTestId('kpi-conversion')).toContainText(/paid of .* closed/);
			expect(await hasHorizontalScroll(admin)).toBe(false);
		});
	});
}
