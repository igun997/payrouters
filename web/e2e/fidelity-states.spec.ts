// State screens (design artboards "Dashboard ... Empty", "Error Banner", "Session Expired", "Skeleton",
// "Not Found", "Toasts"): the structure the design specifies, checked in the real app. Real data is used
// wherever the shared database can produce the state (a filter with no matches, an unknown URL, a
// toast from a real action). Route stubs answer only where a state needs an empty or failing response.
import type { Locator } from '@playwright/test';
import { uid } from './support/api';
import { expect, hasHorizontalScroll, test } from './support/fixtures';
async function box(locator: Locator) {
	const b = await locator.boundingBox();
	if (!b) throw new Error('element has no bounding box');
	return b;
}

/** Box once it stops moving: toasts slide in, so the first read can be mid-animation. */
async function settledBox(locator: Locator) {
	let prev = await box(locator);
	for (let i = 0; i < 30; i++) {
		await locator.page().waitForTimeout(100);
		const next = await box(locator);
		if (next.x === prev.x && next.y === prev.y && next.height === prev.height) return next;
		prev = next;
	}
	return prev;
}

/** A list endpoint answer with no rows, for the unfiltered "no payments yet" state. */
const EMPTY_PAGE = JSON.stringify({ data: [], next_cursor: null });

test.describe('state screens', () => {
	test('an unfiltered empty payments list shows the dashed state and its action, with no table', async ({
		admin
	}) => {
		await admin.route(/\/admin\/payments(\?|$)/, (route) =>
			route.fulfill({ status: 200, contentType: 'application/json', body: EMPTY_PAGE })
		);
		await admin.goto('/payments');

		const state = admin.getByTestId('empty-state');
		await expect(state.getByRole('heading', { name: 'No payments in sandbox yet' })).toBeVisible();
		await expect(state).toContainText('Create a test payment in the Playground');
		await expect(state.getByRole('link', { name: 'Open Playground' })).toHaveAttribute(
			'href',
			/\/playground$/
		);
		await expect(admin.getByRole('table')).toHaveCount(0);
		await expect(admin.getByTestId('payments-list')).toHaveCount(0);
		expect(await hasHorizontalScroll(admin)).toBe(false);
	});

	test('a filter with no matches keeps the controls and offers Clear filters', async ({
		admin
	}) => {
		await admin.goto('/payments');
		await admin.getByTestId('payments-search').fill(uid('NONE'));

		const state = admin.getByTestId('empty-state');
		await expect(
			state.getByRole('heading', { name: 'No payments match these filters' })
		).toBeVisible();
		await expect(state.getByRole('button', { name: 'Clear filters' })).toBeVisible();
		await expect(admin.getByTestId('payments-search')).toHaveValue(/^NONE-/);
		expect(await hasHorizontalScroll(admin)).toBe(false);

		await state.getByRole('button', { name: 'Clear filters' }).click();
		await expect(admin.getByTestId('payments-search')).toHaveValue('');
	});

	test('a failed list request shows the banner with Try again, inside the content', async ({
		admin
	}) => {
		await admin.route(/\/admin\/payments(\?|$)/, (route) =>
			route.fulfill({
				status: 502,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'bad gateway' })
			})
		);
		await admin.goto('/payments');

		const banner = admin.getByTestId('banner');
		await expect(banner).toHaveAttribute('role', 'alert');
		await expect(banner).toContainText('Could not load payments');
		await expect(banner).toContainText('The server returned 502.');
		await expect(banner.getByRole('button', { name: 'Try again' })).toBeVisible();
		// Inside content: below the page heading, never over the sidebar or tab bar.
		const title = await box(admin.getByTestId('page-title'));
		expect((await box(banner)).y).toBeGreaterThan(title.y);
		expect(await hasHorizontalScroll(admin)).toBe(false);
	});

	test('a rejected session opens the session-ended dialog with Sign in again', async ({
		admin
	}) => {
		await admin.route(/\/admin\/payments(\?|$)/, (route) =>
			route.fulfill({
				status: 401,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'unauthorized' })
			})
		);
		await admin.goto('/payments');

		const dialog = admin.getByTestId('dialog-session-expired');
		await expect(dialog).toBeVisible();
		await expect(dialog.getByRole('heading', { name: 'Your session ended' })).toBeVisible();
		await expect(dialog.getByRole('button', { name: 'Sign in again' })).toBeVisible();
	});

	test('a pending overview request shows the skeleton blocks, not spinners', async ({ admin }) => {
		// The request never answers, so the overview stays in its loading state.
		await admin.route(/\/admin\/stats\/summary/, () => {});
		await admin.goto('/');

		const stats = admin.getByTestId('overview-skeleton-stats');
		await expect(stats).toBeVisible();
		await expect(stats.locator('[data-slot="skeleton"]').first()).toBeVisible();
		expect(await admin.locator('[role="progressbar"], .animate-spin').count()).toBe(0);
		expect(await hasHorizontalScroll(admin)).toBe(false);
	});

	test('an unknown admin address shows the not-found state linking back to the overview', async ({
		admin
	}) => {
		await admin.goto(`/no-such-page-${uid('x')}`);

		const state = admin.getByTestId('not-found-state');
		await expect(state.getByRole('heading', { name: 'Page not found' })).toBeVisible();
		await expect(state.getByRole('link', { name: 'Back to Overview' })).toHaveAttribute(
			'href',
			'/'
		);
		expect(await hasHorizontalScroll(admin)).toBe(false);
	});

	test('a success toast sits bottom-centre on desktop', async ({ admin, api, product }) => {
		const paid = await api.paidPayment(product.sandboxKey, 150000);
		await admin.goto(`/payments/${paid.id}`);
		await admin.getByTestId('sync-payment').click();

		const toast = admin.locator('[data-sonner-toast]').filter({
			hasText: 'Payment synced with provider'
		});
		await expect(toast).toBeVisible();
		const viewport = admin.viewportSize()!;
		const b = await settledBox(toast);
		expect(b.y + b.height).toBeGreaterThan(viewport.height - 120);
		expect(Math.abs(b.x + b.width / 2 - viewport.width / 2)).toBeLessThan(4);
	});

	test('@phone a success toast clears the tab bar on phones', async ({
		admin,
		api,
		product
	}, testInfo) => {
		test.skip(testInfo.project.name !== 'phone', 'phone layouts only');
		const paid = await api.paidPayment(product.sandboxKey, 150000);
		await admin.goto(`/payments/${paid.id}`);
		await admin.getByTestId('sync-payment').click();

		const toast = admin.locator('[data-sonner-toast]').filter({
			hasText: 'Payment synced with provider'
		});
		await expect(toast).toBeVisible();
		const tabTop = (await box(admin.getByTestId('tab-bar'))).y;
		const b = await settledBox(toast);
		expect(b.y + b.height).toBeLessThanOrEqual(tabTop + 1);
		expect(await hasHorizontalScroll(admin)).toBe(false);
	});
});

// Phone-only shape checks for the state screens: the empty state fills the card width at 320 and 390.
for (const width of [320, 390] as const) {
	test.describe(`state screens phone ${width}`, () => {
		test.use({ viewport: { width, height: width === 320 ? 568 : 844 } });

		test(`@phone the empty payments state stays inside the phone width at ${width}`, async ({
			admin
		}, testInfo) => {
			test.skip(testInfo.project.name !== 'phone', 'phone layouts only');
			await admin.route(/\/admin\/payments(\?|$)/, (route) =>
				route.fulfill({ status: 200, contentType: 'application/json', body: EMPTY_PAGE })
			);
			await admin.goto('/payments');

			const state = admin.getByTestId('empty-state');
			await expect(
				state.getByRole('heading', { name: 'No payments in sandbox yet' })
			).toBeVisible();
			const b = await box(state);
			expect(b.x).toBeGreaterThanOrEqual(0);
			expect(b.x + b.width).toBeLessThanOrEqual(width + 1);
			expect(await hasHorizontalScroll(admin)).toBe(false);
		});
	});
}
