// API docs: reachable from the sidebar (desktop), from More (phone) and from the Payments empty
// state, with this deployment's origin in the examples and no horizontal page scroll.
import { expect, hasHorizontalScroll, test } from './support/fixtures';

test.describe('API docs', () => {
	test('opens from the sidebar and uses this origin as the base URL', async ({ admin }) => {
		test.skip((admin.viewportSize()?.width ?? 0) < 768, 'phones open it from More');
		await admin.goto('/');
		await admin.getByTestId('sidebar').getByRole('link', { name: 'API docs' }).click();

		await expect(admin).toHaveURL(/\/docs$/);
		await expect(admin.getByTestId('page-title')).toHaveText('API docs');
		const origin = new URL(admin.url()).origin;
		await expect(admin.getByTestId('docs-base-url')).toContainText(origin);
		await expect(admin.getByRole('heading', { name: 'Create a payment' })).toBeVisible();
		await expect(admin.getByRole('heading', { name: 'Webhooks', exact: true })).toBeVisible();
		expect(await hasHorizontalScroll(admin)).toBe(false);
	});

	test('opens from More on a phone @phone', async ({ admin }) => {
		test.skip((admin.viewportSize()?.width ?? 0) >= 768, 'phone only');
		await admin.goto('/');
		await admin.getByTestId('tab-more').click();
		await admin
			.getByTestId('page-more')
			.getByRole('link', { name: /API docs/ })
			.click();

		await expect(admin).toHaveURL(/\/docs$/);
		await expect(admin.getByTestId('shell-back')).toBeVisible();
		await expect(admin.getByRole('heading', { name: 'Create a payment' })).toBeVisible();
		expect(await hasHorizontalScroll(admin)).toBe(false);
	});

	test('the Payments empty state links to the docs', async ({ admin }) => {
		await admin.route(/\/admin\/payments(\?|$)/, (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ data: [], next_cursor: null })
			})
		);
		await admin.goto('/payments');
		await expect(admin.getByTestId('empty-state')).toBeVisible();
		await admin.getByRole('link', { name: 'Read the API docs' }).click();

		await expect(admin).toHaveURL(/\/docs$/);
		await expect(admin.getByTestId('page-docs')).toBeVisible();
	});

	test('the create-payment example is copyable text with the real endpoint', async ({ admin }) => {
		await admin.goto('/docs');
		const block = admin.getByTestId('code-block').filter({ hasText: 'POST' }).first();
		await expect(block).toContainText('/v1/payments');
		await expect(block).toContainText('Authorization: Bearer prk_test_');
	});
});
