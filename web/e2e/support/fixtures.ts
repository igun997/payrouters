import { randomInt } from 'node:crypto';
import { test as base, expect, type Page } from '@playwright/test';
import { ADMIN_TOKEN, Api, TOKEN_KEY, type Product } from './api';

type Fixtures = {
	/**
	 * A client address private to this test. The server keys its per-client limits by
	 * `X-Forwarded-For` (TRUSTED_PROXY_HEADER in start-server.sh), so tests never share a bucket.
	 */
	clientIp: string;
	api: Api;
	/** A page already signed in as admin (token stored before first navigation). */
	admin: Page;
	/** Fresh product with sandbox and live keys. */
	product: Product;
};

/** A random IPv4 address in 10.0.0.0/8 (never a .0 or .255 host). */
export function randomClientIp(): string {
	return `10.${randomInt(256)}.${randomInt(256)}.${randomInt(1, 255)}`;
}

export const test = base.extend<Fixtures>({
	// Playwright requires the first fixture argument to be an object pattern.
	// eslint-disable-next-line no-empty-pattern
	clientIp: async ({}, use) => use(randomClientIp()),
	page: async ({ page, clientIp }, use) => {
		await page.context().setExtraHTTPHeaders({ 'X-Forwarded-For': clientIp });
		await use(page);
	},
	request: async ({ playwright, baseURL, clientIp }, use) => {
		const ctx = await playwright.request.newContext({
			baseURL,
			extraHTTPHeaders: { 'X-Forwarded-For': clientIp }
		});
		await use(ctx);
		await ctx.dispose();
	},
	api: async ({ request }, use) => use(new Api(request)),
	admin: async ({ page }, use) => {
		await page.addInitScript(([k, v]) => localStorage.setItem(k, v), [TOKEN_KEY, ADMIN_TOKEN]);
		await use(page);
	},
	product: async ({ api }, use) => use(await api.createProduct())
});

export { expect };

export const VIEWPORTS = [
	{ name: 'xs-320', width: 320, height: 568 },
	{ name: 'base-390', width: 390, height: 844 },
	{ name: 'md-768', width: 768, height: 1024 },
	{ name: 'lg-1024', width: 1024, height: 768 },
	{ name: 'xl-1366', width: 1366, height: 768 }
] as const;

/** True when the page scrolls horizontally (a layout defect at every supported width). */
export async function hasHorizontalScroll(page: Page): Promise<boolean> {
	return page.evaluate(
		() => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1
	);
}

/** Texts whose amount wrapped onto a second line, e.g. "Rp" alone above the figure. */
export async function wrappedAmounts(page: Page): Promise<string[]> {
	return page.evaluate(() => {
		const out: string[] = [];
		const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
		for (let n = walker.nextNode(); n; n = walker.nextNode()) {
			const text = n.textContent?.trim() ?? '';
			if (!/^(−\s?)?Rp\s?[\d.]+$/.test(text)) continue;
			const range = document.createRange();
			range.selectNodeContents(n);
			const rects = [...range.getClientRects()];
			if (rects.length > 1 && new Set(rects.map((r) => Math.round(r.top))).size > 1) out.push(text);
		}
		return out;
	});
}
