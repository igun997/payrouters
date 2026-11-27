# E2E testing (Playwright)

Real backend, real Postgres, built SPA: nothing is mocked except external providers (the `mock` provider and signed webhook posts stand in for Midtrans/Xendit).

```bash
docker compose up -d db                      # Postgres on 127.0.0.1:5435
cd web && pnpm build                         # SPA -> web/build
rcargo --pull build -p payrouters-server     # server binary -> target/debug/payrouters-server
pnpm exec playwright install chromium        # once
pnpm test:e2e                                # desktop 1366x768 (+ @phone specs on 390x844)
E2E_PORT=8093 pnpm exec playwright test e2e/snap.spec.ts   # own port + own database (parallel agents)
```

- `e2e/support/start-server.sh` drops and recreates database `payrouters_e2e_<port>` and execs the server with `WEB_DIR=web/build`; Playwright's `webServer` waits on `/healthz`. It sets `TRUSTED_PROXY_HEADER=x-forwarded-for` and generous `RATE_*` request limits; the failed-sign-in lockout keeps its default.
- `e2e/support/api.ts` arranges state through the admin/product APIs (products, keys, providers, paid payments, signed Midtrans notifications); specs assert through the UI.
- `e2e/support/fixtures.ts`: `api`, `admin` (signed-in page), `product`, `clientIp`, `VIEWPORTS`, `hasHorizontalScroll`, `wrappedAmounts`. Every page and `request` in a test sends that test's own `X-Forwarded-For` (`clientIp`), so per-client limits never mix tests.
- Selectors: `getByRole`/`getByLabel` first, `data-testid` for structural hooks (listed in the builders' notes: `page-<name>`, `<thing>-list|row`, `dialog-*`, `snap-*`).
- Last full run: 241 passed, 31 skipped across the desktop and phone projects (a test that needs the other viewport skips itself).

Specs: login, rate-limit (sign-in lockout, login copy), snap (all states), payments, products, providers, webhooks, cash-ins, playground, docs, live-mode, responsive (320/390/768/1024/1366), a11y (axe, serious+critical).
