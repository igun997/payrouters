// Per-client sign-in lockout and the login page's wording. Every test runs as its own client
// address (fixtures.ts), so the 10-failure limit here never touches another test.
import { ADMIN_TOKEN } from './support/api';
import { expect, randomClientIp, test } from './support/fixtures';

const WRONG = 'not-the-admin-token';
const FORBIDDEN = [/ADMIN_TOKEN/, /env/i, /server/i];

test.describe('sign-in rate limit', () => {
	test('ten wrong tokens lock the form; the eleventh attempt shows the wait time', async ({
		page,
		request
	}) => {
		await page.goto('/login');
		const input = page.getByTestId('token-input');
		const signIn = page.getByRole('button', { name: 'Sign in' });

		for (let attempt = 1; attempt <= 10; attempt++) {
			await input.fill(WRONG);
			const reply = page.waitForResponse((r) => r.url().includes('/admin/me'));
			await signIn.click();
			expect((await reply).status(), `attempt ${attempt}`).toBe(401);
			await expect(page.getByRole('alert')).toContainText('That token was not accepted');
		}

		await input.fill(WRONG);
		const reply = page.waitForResponse((r) => r.url().includes('/admin/me'));
		await signIn.click();
		const res = await reply;
		expect(res.status()).toBe(429);
		expect(res.headers()['retry-after']).toMatch(/^[1-9]\d*$/);

		const alert = page.getByRole('alert');
		await expect(alert).toContainText('Too many attempts. Try again in');
		const shown = Number(
			/Try again in (\d+) seconds\./.exec((await alert.textContent()) ?? '')?.[1]
		);
		expect(shown).toBeGreaterThan(0);
		expect(shown).toBeLessThanOrEqual(300);
		await expect(signIn).toBeDisabled();
		await expect(alert).not.toContainText('That token was not accepted');

		// The right token from the same client is refused for the rest of the window. Asserted
		// on the API because the form itself is closed while locked.
		const refused = await request.get('/admin/me', {
			headers: { Authorization: `Bearer ${ADMIN_TOKEN}` }
		});
		expect(refused.status()).toBe(429);
		const body = (await refused.json()) as { error: { code: string } };
		expect(body.error.code).toBe('rate_limited');

		// Other clients are untouched: a wrong token is still a plain 401, the right one still works.
		const other = { 'X-Forwarded-For': randomClientIp() };
		const wrong = await request.get('/admin/me', {
			headers: { Authorization: `Bearer ${WRONG}`, ...other }
		});
		expect(wrong.status()).toBe(401);
		const right = await request.get('/admin/me', {
			headers: { Authorization: `Bearer ${ADMIN_TOKEN}`, ...other }
		});
		expect(right.status()).toBe(200);
	});
});

for (const viewport of [
	{ width: 390, height: 844 },
	{ width: 1366, height: 768 }
]) {
	test(`login copy names no server configuration at ${viewport.width}px`, async ({ page }) => {
		await page.setViewportSize(viewport);
		await page.goto('/login');
		const main = page.getByTestId('page-login');
		await expect(main).toBeVisible();

		const text = await main.innerText();
		const html = await main.evaluate((el) => el.outerHTML);
		const title = await page.title();
		for (const pattern of FORBIDDEN) {
			expect(text, `visible text matches ${pattern}`).not.toMatch(pattern);
			expect(html, `markup matches ${pattern}`).not.toMatch(pattern);
			expect(title, `title matches ${pattern}`).not.toMatch(pattern);
		}
		await expect(page.getByLabel('Admin token')).toBeVisible();
	});
}
