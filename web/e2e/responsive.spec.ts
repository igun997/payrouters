// Responsive sweep: every design width x every key page, on seeded data. One test per (page, width).
import { type APIRequestContext, type Page, expect } from '@playwright/test';
import { Api, type Product, uid } from './support/api';
import { hasHorizontalScroll, test, VIEWPORTS, wrappedAmounts } from './support/fixtures';
import { BASE_URL, overflowingRight, signIn, visibleTextNodes } from './support/layout-helpers';
import { newSnapPayment } from './support/snap-helpers';

// Type scale per viewport, in VIEWPORTS order (320, 390, 768, 1024, 1366). Source: docs/design.md section 2.
const STAT = [22, 24, 24, 24, 24];
const PAGE_TITLE = [20, 22, 24, 24, 24];
const RESULT_TITLE = [24, 28, 28, 28, 28];
const AMOUNT_XL = [36, 44, 44, 44, 44];
const AMOUNT_LG = [28, 34, 36, 36, 36];
const MONO_VA = [22, 28, 28, 28, 28];

/** Seeded ids by slot name. Paths use `:slot` placeholders (for example `/snap/:va`). */
type Slots = Record<string, string>;

/** What the seeding returns: the product and the slot ids the routes need. */
interface Seeded {
	product: Product;
	slots: Slots;
}

async function seedSlots(api: Api): Promise<Seeded> {
	const product = await api.createProduct(uid('Shop'), 'https://hooks.example.test/payrouters');
	const midtrans = await api.ensureProvider('midtrans');
	await api.midtransNotification(midtrans, uid('UNMATCHED'), 150000);
	for (const amount of [150000, 2500000, 12750000]) {
		await api.paidPayment(product.sandboxKey, amount, 'va_bca');
	}
	// Paid Rp 48.250.000: the detail page and the widest figure in the sweep.
	const big = await api.paidPayment(product.sandboxKey, 48250000, 'va_bca');
	const choose = await newSnapPayment(api, product.sandboxKey);
	const va = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
	const qris = await newSnapPayment(api, product.sandboxKey, { method: 'qris' });
	const expired = await newSnapPayment(api, product.sandboxKey, { method: 'va_bca' });
	expect(await api.simulate(expired.token, 'expired')).toBe(200);
	return {
		product,
		slots: {
			paidId: big.id,
			choose: choose.token,
			va: va.token,
			qris: qris.token,
			paid: big.token,
			expired: expired.token,
			missing: uid('missing')
		}
	};
}

type Kind = 'login' | 'admin' | 'snap';

/** One computed font-size assertion: `target` must have `sizes[viewport index]`. */
interface FontCheck {
	what: string;
	target: string;
	sizes: readonly number[];
}

interface Scenario {
	name: string;
	kind: Kind;
	/** Route with `:slot` placeholders. */
	path: string;
	/** Selector that is visible once the page has its data. */
	ready: string;
	fonts: FontCheck[];
	/** Snap Choose and Paid must fit 1366x768 without vertical scroll. */
	fitsDesktop?: boolean;
	/** Snap method rows are touch targets below md. */
	touchRows?: boolean;
}

const PAGE_TITLE_CHECK: FontCheck = {
	what: 'page title',
	target: '[data-testid="shell-title"]',
	sizes: PAGE_TITLE
};
const SNAP_RESULT_TITLE: FontCheck = {
	what: 'result title',
	target: '[data-testid="snap-status"] h1',
	sizes: RESULT_TITLE
};

const SCENARIOS: Scenario[] = [
	{
		name: 'Login',
		kind: 'login',
		path: '/login',
		ready: '[data-testid="token-input"]',
		fonts: [{ what: 'login title', target: '[data-testid="page-title"]', sizes: RESULT_TITLE }]
	},
	{
		name: 'Overview',
		kind: 'admin',
		path: '/',
		ready: '[data-testid="kpi-conversion"]',
		fonts: [
			PAGE_TITLE_CHECK,
			{ what: 'stat figure', target: '[data-testid="kpi-conversion"] .text-stat', sizes: STAT }
		]
	},
	{
		name: 'Payments',
		kind: 'admin',
		path: '/payments',
		ready: '[data-testid="payment-row"]',
		fonts: [PAGE_TITLE_CHECK]
	},
	{
		name: 'Payment detail',
		kind: 'admin',
		path: '/payments/:paidId',
		ready: '[data-testid="payment-order-id"]',
		fonts: [PAGE_TITLE_CHECK]
	},
	{
		name: 'Cash-ins',
		kind: 'admin',
		path: '/cash-ins',
		ready: '[data-testid="cash-in-totals"]',
		fonts: [PAGE_TITLE_CHECK]
	},
	{
		name: 'Products',
		kind: 'admin',
		path: '/products',
		ready: '[data-testid="product-list"]',
		fonts: [PAGE_TITLE_CHECK]
	},
	{
		name: 'Providers',
		kind: 'admin',
		path: '/providers',
		ready: '[data-testid="provider-list"]',
		fonts: [PAGE_TITLE_CHECK]
	},
	{
		name: 'Webhooks',
		kind: 'admin',
		path: '/webhooks',
		ready: '[data-testid="webhook-event-row"]',
		fonts: [PAGE_TITLE_CHECK]
	},
	{
		name: 'Playground',
		kind: 'admin',
		path: '/playground',
		ready: '[data-testid="playground-form"]',
		fonts: [PAGE_TITLE_CHECK]
	},
	{
		name: 'Snap Choose',
		kind: 'snap',
		path: '/snap/:choose',
		ready: '[data-testid="snap-methods"]',
		fonts: [
			{ what: 'choose amount', target: '[data-testid="snap-amount"]', sizes: AMOUNT_XL },
			{ what: 'choose title', target: '[data-testid="snap-methods"] h1', sizes: PAGE_TITLE }
		],
		fitsDesktop: true,
		touchRows: true
	},
	{
		name: 'Snap VA',
		kind: 'snap',
		path: '/snap/:va',
		ready: '[data-testid="snap-va"]',
		fonts: [
			{ what: 'VA amount', target: '[data-testid="snap-amount"]', sizes: AMOUNT_LG },
			{ what: 'VA number', target: '[data-testid="snap-account-number"]', sizes: MONO_VA }
		]
	},
	{
		name: 'Snap QRIS',
		kind: 'snap',
		path: '/snap/:qris',
		ready: '[data-testid="snap-qris"]',
		fonts: [{ what: 'QRIS amount', target: '[data-testid="snap-amount"]', sizes: AMOUNT_LG }]
	},
	{
		name: 'Snap Paid',
		kind: 'snap',
		path: '/snap/:paid',
		ready: '[data-testid="snap-status"][data-status="paid"]',
		fonts: [
			SNAP_RESULT_TITLE,
			{ what: 'paid amount', target: '[data-testid="snap-status"] .text-stat', sizes: STAT }
		],
		fitsDesktop: true
	},
	{
		name: 'Snap Expired',
		kind: 'snap',
		path: '/snap/:expired',
		ready: '[data-testid="snap-status"][data-status="expired"]',
		fonts: [SNAP_RESULT_TITLE]
	},
	{
		name: 'Snap Not Found',
		kind: 'snap',
		path: '/snap/:missing',
		ready: '[data-testid="snap-status"][data-status="not_found"]',
		fonts: [SNAP_RESULT_TITLE]
	}
];

/** Replaces each `:slot` in a route with the seeded id. */
function fillPath(path: string, slots: Slots): string {
	return path.replace(/:(\w+)/g, (_, slot: string) => slots[slot] ?? '');
}

/** Below md: bottom tab bar with five items, no sidebar. md: 72px icon rail with no labels. lg and up: 248px sidebar. */
async function expectAdminNav(page: Page, width: number): Promise<void> {
	const tabBar = page.getByTestId('tab-bar');
	const sidebar = page.getByTestId('sidebar');
	if (width < 768) {
		await expect(tabBar).toBeVisible();
		await expect(sidebar).toBeHidden();
		const items = tabBar.locator('a, button');
		await expect(items).toHaveCount(5);
		const heights = await items.evaluateAll((els) =>
			els.map((el) => el.getBoundingClientRect().height)
		);
		for (const h of heights) expect(h, 'tab target height').toBeGreaterThanOrEqual(44);
		return;
	}
	await expect(tabBar).toBeHidden();
	await expect(sidebar).toBeVisible();
	const box = await sidebar.boundingBox();
	expect(box?.width, 'sidebar width').toBeCloseTo(width < 1024 ? 72 : 248, 0);
	if (width < 1024) {
		expect(await visibleTextNodes(sidebar), 'icon rail labels').toBe(0);
	} else {
		await expect(sidebar.getByRole('link', { name: 'Overview' })).toBeVisible();
		await expect(sidebar.getByRole('link', { name: 'Playground' })).toBeVisible();
	}
}

for (const [index, vp] of VIEWPORTS.entries()) {
	test.describe(`viewport ${vp.width}`, () => {
		test.use({ viewport: { width: vp.width, height: vp.height } });

		let client: APIRequestContext;
		let seeded: Seeded;

		test.beforeAll(async ({ playwright }) => {
			client = await playwright.request.newContext({ baseURL: BASE_URL });
			seeded = await seedSlots(new Api(client));
		});

		test.afterAll(async () => {
			await client.dispose();
		});

		for (const scenario of SCENARIOS) {
			test(`${scenario.name} @ ${vp.width}`, async ({ page }) => {
				if (scenario.kind === 'admin') await signIn(page);
				await page.goto(fillPath(scenario.path, seeded.slots));
				await expect(page.locator(scenario.ready).first()).toBeVisible();
				await page.evaluate(() => document.fonts.ready);

				expect(await hasHorizontalScroll(page), 'horizontal scroll').toBe(false);
				expect(await wrappedAmounts(page), 'amounts wrapped onto a second line').toEqual([]);
				expect(await overflowingRight(page), 'content past the right edge').toEqual([]);

				if (scenario.kind === 'admin') await expectAdminNav(page, vp.width);

				if (scenario.touchRows && vp.width < 768) {
					const heights = await page
						.locator('[data-testid^="snap-method-"]')
						.evaluateAll((els) => els.map((el) => el.getBoundingClientRect().height));
					expect(heights.length, 'method rows').toBe(6);
					for (const h of heights) expect(h, 'method row height').toBeGreaterThanOrEqual(44);
				}

				if (scenario.fitsDesktop && vp.width === 1366) {
					const fits = await page.evaluate(
						() => document.documentElement.scrollHeight <= window.innerHeight + 1
					);
					expect(fits, 'no vertical scroll at 1366x768').toBe(true);
				}

				for (const check of scenario.fonts) {
					const locator = page.locator(check.target).first();
					await expect(locator, check.what).toBeVisible();
					const size = await locator.evaluate((el) => parseFloat(getComputedStyle(el).fontSize));
					expect(size, `${check.what} font-size at ${vp.width}`).toBe(check.sizes[index]);
				}
			});
		}
	});
}
