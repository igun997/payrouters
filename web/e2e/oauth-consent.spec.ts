// MCP OAuth consent (/oauth/authorize): the consent card, Allow with a narrowed scope, Deny, a bad
// redirect_uri, the sign-in round trip, and the layout at every width. Specs drive the real
// authorization server: registration, the browser decision, the token exchange and /mcp.
import { ADMIN_TOKEN, uid } from './support/api';
import { expect, hasHorizontalScroll, test, VIEWPORTS } from './support/fixtures';
import {
	callTool,
	exchangeCode,
	interceptCallback,
	mcp,
	newAuthorization,
	INITIALIZE_PARAMS,
	waitForCallback
} from './support/oauth';

test.describe('OAuth consent', () => {
	test('Allow with write unticked issues a read-only token', async ({
		admin,
		request,
		baseURL
	}) => {
		const auth = await newAuthorization(request, baseURL);
		await interceptCallback(admin);
		await admin.goto(auth.path);

		await expect(admin.getByTestId('page-title')).toHaveText(
			`Connect ${auth.client.client_name} to PayRouters`
		);
		await expect(admin.getByTestId('scope-read')).toBeChecked();
		await expect(admin.getByTestId('scope-read')).toBeDisabled();
		await expect(admin.getByTestId('scope-write')).toBeChecked();
		await expect(admin.getByTestId('scope-live')).not.toBeChecked();

		await admin.getByTestId('scope-write').uncheck();
		await admin.getByTestId('oauth-allow').click();

		const callback = await waitForCallback(admin);
		expect(callback.searchParams.get('state')).toBe(auth.state);
		const code = callback.searchParams.get('code');
		expect(code).toBeTruthy();

		const token = await exchangeCode(request, auth, code!);
		expect(token.status).toBe(200);
		expect(token.json.scope).toBe('read');
		const accessToken = token.json.access_token;

		const init = await mcp(request, accessToken, 'initialize', INITIALIZE_PARAMS);
		expect(init.status).toBe(200);

		const tools = await mcp(request, accessToken, 'tools/list');
		const names = tools.body?.result?.tools?.map((tool) => tool.name) ?? [];
		expect(names).toEqual(expect.arrayContaining(['get_integration_status', 'list_products']));

		const read = await callTool(request, accessToken, 'list_products', {});
		expect(read.body?.result?.isError).toBe(false);

		const write = await callTool(request, accessToken, 'create_product', { name: uid('Shop') });
		expect(write.body?.result?.isError).toBe(true);
	});

	test('Deny redirects with access_denied and the same state', async ({
		admin,
		request,
		baseURL
	}) => {
		const auth = await newAuthorization(request, baseURL);
		await interceptCallback(admin);
		await admin.goto(auth.path);

		await expect(admin.getByTestId('oauth-deny')).toBeVisible();
		await admin.getByTestId('oauth-deny').click();

		const callback = await waitForCallback(admin);
		expect(callback.searchParams.get('error')).toBe('access_denied');
		expect(callback.searchParams.get('state')).toBe(auth.state);
		expect(callback.searchParams.get('code')).toBeNull();
	});

	test('an unregistered redirect_uri shows the error card and never navigates away', async ({
		admin,
		request,
		baseURL
	}) => {
		const auth = await newAuthorization(request, baseURL, {
			redirectUri: 'https://evil.example.com/callback'
		});
		await admin.goto(auth.path);

		await expect(admin.getByTestId('oauth-error')).toBeVisible();
		await expect(admin.getByTestId('oauth-allow')).toHaveCount(0);
		await expect(admin.getByTestId('oauth-dashboard-link')).toBeVisible();
		expect(new URL(admin.url()).origin).toBe(new URL(baseURL ?? '').origin);
		expect(new URL(admin.url()).pathname).toBe('/oauth/authorize');
	});

	test('without a session the page goes to sign-in and returns to the same consent request', async ({
		page,
		request,
		baseURL
	}) => {
		const auth = await newAuthorization(request, baseURL);
		await interceptCallback(page);
		await page.goto(auth.path);

		await expect(page).toHaveURL(/\/login\?next=%2Foauth%2Fauthorize%3F/);
		await page.getByTestId('token-input').fill(ADMIN_TOKEN);
		await page.getByRole('button', { name: 'Sign in' }).click();

		await expect(page).toHaveURL(/\/oauth\/authorize\?/);
		await expect(page.getByTestId('oauth-allow')).toBeVisible();
		expect(new URL(page.url()).searchParams.get('state')).toBe(auth.state);

		await page.getByTestId('oauth-allow').click();
		const callback = await waitForCallback(page);
		expect(callback.searchParams.get('state')).toBe(auth.state);
		expect(callback.searchParams.get('code')).toBeTruthy();
	});

	test('the consent card fits every width without horizontal scroll', async ({
		admin,
		request,
		baseURL
	}) => {
		const auth = await newAuthorization(request, baseURL);
		for (const viewport of VIEWPORTS) {
			await admin.setViewportSize({ width: viewport.width, height: viewport.height });
			await admin.goto(auth.path);
			await expect(admin.getByTestId('oauth-allow')).toBeVisible();
			expect(await hasHorizontalScroll(admin), viewport.name).toBe(false);
		}
	});

	test('the Allow and Deny buttons take full width on a phone @phone', async ({
		admin,
		request,
		baseURL
	}) => {
		test.skip((admin.viewportSize()?.width ?? 0) >= 640, 'phone layout only');
		const auth = await newAuthorization(request, baseURL);
		await admin.goto(auth.path);

		const allow = admin.getByTestId('oauth-allow');
		await expect(allow).toBeVisible();
		const box = await allow.boundingBox();
		const viewport = admin.viewportSize();
		expect(box?.width ?? 0).toBeGreaterThan((viewport?.width ?? 0) * 0.7);
		expect(box?.height ?? 0).toBeGreaterThanOrEqual(44);
	});
});
