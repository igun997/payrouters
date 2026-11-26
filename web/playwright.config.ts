import { defineConfig, devices } from '@playwright/test';

const PORT = Number(process.env.E2E_PORT ?? 8091);

/**
 * Viewports are the design's reference sizes (docs/design.md): 320 XS, 390 Base, 768 md,
 * 1024 lg, 1366 xl. Functional specs run on desktop (chromium 1366x768) and phone (390x844);
 * `responsive.spec.ts` sweeps every width itself.
 */
export default defineConfig({
	testDir: 'e2e',
	timeout: 30_000,
	expect: { timeout: 7_000 },
	fullyParallel: false, // one shared server and database; specs create their own data with unique ids
	workers: 1,
	retries: process.env.CI ? 1 : 0,
	reporter: [['list'], ['html', { open: 'never', outputFolder: 'e2e-report' }]],
	outputDir: 'e2e-results',
	use: {
		// Full Chromium in new headless mode: avoids the separate headless-shell download.
		channel: 'chromium',
		baseURL: `http://127.0.0.1:${PORT}`,
		trace: 'retain-on-failure',
		screenshot: 'only-on-failure',
		locale: 'en-US',
		timezoneId: 'Asia/Jakarta'
	},
	projects: [
		{
			name: 'desktop',
			use: { ...devices['Desktop Chrome'], viewport: { width: 1366, height: 768 } }
		},
		{
			name: 'phone',
			use: { ...devices['Pixel 7'], viewport: { width: 390, height: 844 } },
			grep: /@phone/
		}
	],
	webServer: {
		command: './e2e/support/start-server.sh',
		url: `http://127.0.0.1:${PORT}/healthz`,
		reuseExistingServer: !process.env.CI,
		timeout: 60_000,
		stdout: 'pipe',
		stderr: 'pipe'
	}
});
