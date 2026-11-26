// Layout and sign-in helpers for the responsive and accessibility specs.
import type { Locator, Page } from '@playwright/test';
import { ADMIN_TOKEN, TOKEN_KEY } from './api';

/** Same origin the Playwright config uses (E2E_PORT, default 8091). */
export const BASE_URL = `http://127.0.0.1:${process.env.E2E_PORT ?? 8091}`;

/** Stores the admin token before the first navigation, so admin routes render signed in. */
export async function signIn(page: Page): Promise<void> {
	await page.addInitScript(([k, v]) => localStorage.setItem(k, v), [TOKEN_KEY, ADMIN_TOKEN]);
}

/**
 * Visible elements whose right edge runs past the viewport. Elements inside an intentional
 * horizontal scroller (`data-scroll-x`) are ignored. Returns a short description per element.
 */
export async function overflowingRight(page: Page): Promise<string[]> {
	return page.evaluate(() => {
		const limit = window.innerWidth + 1;
		const out: string[] = [];
		for (const el of document.body.querySelectorAll<HTMLElement>('*')) {
			if (el.closest('[data-scroll-x]')) continue;
			const style = getComputedStyle(el);
			if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') {
				continue;
			}
			const rect = el.getBoundingClientRect();
			if (rect.width === 0 || rect.height === 0) continue;
			if (rect.right > limit) {
				const id = el.dataset.testid ? `[data-testid=${el.dataset.testid}]` : '';
				out.push(`${el.tagName.toLowerCase()}${id} right=${Math.round(rect.right)}`);
			}
		}
		return out;
	});
}

/** Number of visible text-bearing spans or paragraphs inside a locator (proves a rail has no labels). */
export async function visibleTextNodes(locator: Locator): Promise<number> {
	return locator.locator('span, p').filter({ hasText: /\S/ }).filter({ visible: true }).count();
}
