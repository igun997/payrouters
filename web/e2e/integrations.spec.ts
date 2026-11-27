// Integrations: the MCP server URL and client snippets, the capability list, connected apps (approved
// through the consent screen, then revoked, after which the token answers 401), and the page's
// layout and error states. Reachable from the sidebar (desktop) and from More (phone).
import { expect, hasHorizontalScroll, test, VIEWPORTS } from './support/fixtures';
import {
	exchangeCode,
	interceptCallback,
	mcp,
	newAuthorization,
	waitForCallback
} from './support/oauth';

test.describe('Integrations', () => {
	test('shows the server URL and the four client snippets', async ({ admin, baseURL }) => {
		await admin.goto('/integrations');
		await expect(admin.getByTestId('page-integrations')).toBeVisible();
		await expect(admin.getByTestId('page-title')).toHaveText('Integrations');

		const url = `${new URL(baseURL ?? '').origin}/mcp`;
		await expect(admin.getByTestId('mcp-url')).toContainText(url);
		await expect(admin.getByTestId('snippet-claude-code')).toContainText(
			`claude mcp add --transport http payrouters ${url}`
		);
		await expect(admin.getByTestId('client-claude-desktop')).toContainText('Add custom connector');
		await expect(admin.getByTestId('snippet-cursor')).toContainText(`"url": "${url}"`);
		await expect(admin.getByTestId('snippet-vscode')).toContainText('"type": "http"');
		await expect(admin.getByTestId('snippet-vscode')).toContainText(`"url": "${url}"`);
	});

	test('opens from the sidebar on desktop', async ({ admin }) => {
		test.skip((admin.viewportSize()?.width ?? 0) < 768, 'phones open it from More');
		await admin.goto('/');
		await admin.getByTestId('sidebar').getByRole('link', { name: 'Integrations' }).click();

		await expect(admin).toHaveURL(/\/integrations$/);
		await expect(admin.getByTestId('page-integrations')).toBeVisible();
		expect(await hasHorizontalScroll(admin)).toBe(false);
	});

	test('opens from More on a phone @phone', async ({ admin }) => {
		test.skip((admin.viewportSize()?.width ?? 0) >= 768, 'phone only');
		await admin.goto('/');
		await admin.getByTestId('tab-more').click();
		await admin
			.getByTestId('page-more')
			.getByRole('link', { name: /Integrations/ })
			.click();

		await expect(admin).toHaveURL(/\/integrations$/);
		await expect(admin.getByTestId('shell-back')).toBeVisible();
		await expect(admin.getByTestId('page-integrations')).toBeVisible();
	});

	test('the page fits every width without horizontal scroll', async ({ admin }) => {
		for (const viewport of VIEWPORTS) {
			await admin.setViewportSize({ width: viewport.width, height: viewport.height });
			await admin.goto('/integrations');
			await expect(admin.getByTestId('mcp-url')).toBeVisible();
			expect(await hasHorizontalScroll(admin), viewport.name).toBe(false);
		}
	});

	test('an approved app is listed, and revoking it stops its token', async ({
		admin,
		request,
		baseURL
	}) => {
		// Approve a client through the consent screen (write included, the default).
		const auth = await newAuthorization(request, baseURL);
		await interceptCallback(admin);
		await admin.goto(auth.path);
		await expect(admin.getByTestId('oauth-allow')).toBeVisible();
		await admin.getByTestId('oauth-allow').click();
		const callback = await waitForCallback(admin);
		const token = await exchangeCode(request, auth, callback.searchParams.get('code') ?? '');
		expect(token.status).toBe(200);
		const accessToken = token.json.access_token;
		expect((await mcp(request, accessToken, 'tools/list')).status).toBe(200);

		await admin.goto('/integrations');
		const row = admin.getByTestId('connection-row').filter({ hasText: auth.client.client_name });
		await expect(row).toBeVisible();
		await expect(row).toContainText('Read');
		await expect(row).toContainText('Make changes');

		await row.getByTestId('revoke-connection').click();
		const dialog = admin.getByTestId('dialog-revoke-connection');
		await expect(dialog).toBeVisible();
		await expect(dialog).toContainText(`Revoke ${auth.client.client_name}?`);
		await dialog.getByRole('button', { name: 'Revoke', exact: true }).click();

		await expect(dialog).toBeHidden();
		await expect(
			admin.getByTestId('connection-row').filter({ hasText: auth.client.client_name })
		).toHaveCount(0);

		const after = await mcp(request, accessToken, 'tools/list');
		expect(after.status).toBe(401);
	});

	test('an empty connection list shows the empty state', async ({ admin }) => {
		await admin.route(/\/admin\/oauth\/grants(\?|$)/, (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ data: [] })
			})
		);
		await admin.goto('/integrations');
		await expect(admin.getByTestId('empty-state')).toContainText('No connected apps');
	});

	test('a failed connection list shows the error banner', async ({ admin }) => {
		await admin.route(/\/admin\/oauth\/grants(\?|$)/, (route) =>
			route.fulfill({
				status: 500,
				contentType: 'application/json',
				body: JSON.stringify({ error: { code: 'internal', message: 'Something broke' } })
			})
		);
		await admin.goto('/integrations');
		await expect(admin.getByRole('alert')).toContainText('Could not load connected apps');
	});

	test('a live approval shows Live mode on the connection', async ({ admin, request, baseURL }) => {
		const auth = await newAuthorization(request, baseURL, { scope: 'read live' });
		await interceptCallback(admin);
		await admin.goto(auth.path);
		await expect(admin.getByTestId('scope-live')).toBeChecked();
		await expect(admin.getByTestId('scope-write')).not.toBeChecked();
		await admin.getByTestId('oauth-allow').click();
		const callback = await waitForCallback(admin);
		// a connection exists once the client has traded its code for a token
		const token = await exchangeCode(request, auth, callback.searchParams.get('code') ?? '');
		expect(token.status).toBe(200);
		expect(token.json.scope).toBe('read live');

		await admin.goto('/integrations');
		const row = admin.getByTestId('connection-row').filter({ hasText: auth.client.client_name });
		await expect(row).toBeVisible();
		await expect(row).toContainText('Live mode');
		await expect(row).not.toContainText('Make changes');
	});
});
