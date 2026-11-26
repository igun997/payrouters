# @payrouters/sdk

Typed client for the PayRouters product API (`/v1`) and webhook signature verification.

- Zero runtime dependencies.
- Uses only the global `fetch` and Web Crypto (`crypto.subtle`), so it runs on Node 18+, Bun, Deno, Cloudflare Workers and browsers.
- Ships ESM and CommonJS builds, each with type declarations. Both work under `node16`, `nodenext` and `bundler` module resolution.

## Install

```sh
pnpm add @payrouters/sdk
# or: npm install @payrouters/sdk · bun add @payrouters/sdk
```

Type declarations use the DOM `fetch`/`Headers` types. Projects on Node need `@types/node` 18 or later (or `lib: ["DOM"]`).

## Create a client

```ts
import { PayRouters } from '@payrouters/sdk';

const pr = new PayRouters({
  apiKey: process.env.PAYROUTERS_API_KEY!,   // prk_test_… (sandbox) or prk_live_… (live)
  baseUrl: 'https://pay.example.com',         // origin of your PayRouters deployment
  timeoutMs: 15_000,                          // optional, per attempt (default 15000)
  // fetch: customFetch,                      // optional, default global fetch
});
```

## Payments

```ts
const { payment, replayed } = await pr.payments.create({
  order_id: 'INV-2026-0001',                  // unique per product and mode, ≤ 64 chars
  amount: 150_000,                            // integer rupiah
  provider: 'midtrans',                       // optional; default is the product's default
  checkout: 'snap',                           // 'snap' (default) | 'provider'
  method: 'va_bca',                           // optional; with 'snap' creates the charge now
  description: 'Order #1',
  customer: { name: 'Budi', email: 'budi@example.com', phone: '+628123456789' },
  items: [{ id: 'sku-1', name: 'Pro plan', price: 150_000, quantity: 1 }],
  metadata: { user_id: '42' },
  expires_in: 3600,                           // seconds, default 86400, minimum 300
  return_url: 'https://app.example.com/thanks',
});

if (payment.instructions?.type === 'redirect') {
  // send the customer to payment.instructions.url
}

const total = payment.payable_amount ?? payment.amount; // always show this as "Total to pay"

const fetched = await pr.payments.get(payment.id);
const page = await pr.payments.list({ order_id: 'INV-2026-0001', status: 'paid', limit: 50 });
// page.data, page.next_cursor

for await (const p of pr.payments.listAll({ status: 'paid' })) {
  // follows next_cursor across pages
}

const synced = await pr.payments.sync(payment.id); // re-reads the provider

const catalog = await pr.methods.list();          // { data: [{ provider, checkout_provider, methods: [...] }] }
```

### Idempotency and retries

- `payments.create` is **never retried automatically**. A POST may have reached the server even when the response was lost.
- The server deduplicates on `order_id`. To recover from an ambiguous failure, call `create` again with the **same** `order_id` and `amount`. If the original payment is still `pending` for the same provider, the server returns it with `replayed: true` (HTTP 200 and `PayRouters-Idempotent-Replay: true`). Any other duplicate fails with `PayRoutersError` code `order_id_conflict` (409).
- GET requests (`get`, `list`, `listAll`, `methods.list`) are retried up to 2 more times on network errors, 5xx and 429, with 250 ms then 500 ms backoff. Tune with `maxRetries` and `retryDelayMs`.

## Errors

```ts
import { PayRoutersError, PayRoutersNetworkError } from '@payrouters/sdk';

try {
  await pr.payments.create(params);
} catch (error) {
  if (error instanceof PayRoutersError) {
    // error.status, error.code (e.g. 'order_id_conflict'), error.message, error.details, error.requestId
  } else if (error instanceof PayRoutersNetworkError) {
    // error.kind: 'timeout' | 'transport'
  } else {
    throw error;
  }
}
```

Non-2xx responses are mapped from the `{ "error": { "code", "message", "details" } }` envelope. `requestId` comes from the `x-request-id` header when present.

## Webhooks

PayRouters signs each delivery with `PayRouters-Signature: t=<unix_seconds>,v1=<hex>`, where `v1 = HMAC-SHA256(webhook_secret, "<t>." + raw body)`. Verify against the **raw body string exactly as received**. Do not verify a re-serialized `JSON.stringify(parsed)`.

`constructEvent` verifies the signature and parses the body:

```ts
import { constructEvent, WebhookSignatureError } from '@payrouters/sdk';

const event = await constructEvent(rawBody, header, process.env.PAYROUTERS_WEBHOOK_SECRET!);
// event.type: 'payment.paid' | 'payment.expired' | 'payment.failed' | 'payment.cancelled'
// event.data.payment: Payment
```

Use `verifyWebhookSignature` when you only need the check:

```ts
await verifyWebhookSignature({
  payload: rawBody,
  header,
  secret,
  toleranceSeconds: 300, // default
  now: () => Math.floor(Date.now() / 1000), // default
});
```

Both throw `WebhookSignatureError` with a `reason` of `missing_header`, `malformed_header`, `no_signature`, `timestamp_out_of_tolerance` or `signature_mismatch`. Stale timestamps (more than 300 s from now) are rejected. Multiple `v1=` entries are accepted, so key rotation works.

Delivery is **at-least-once**. Respond `2xx` once processed, and dedupe on the `PayRouters-Event-Id` header (the event `id`).

### Node: Express with a raw body

```ts
import express from 'express';
import { constructEvent, WebhookSignatureError } from '@payrouters/sdk';

const app = express();

app.post(
  '/webhooks/payrouters',
  express.raw({ type: 'application/json' }), // req.body is a Buffer of the exact bytes
  async (req, res) => {
    try {
      const event = await constructEvent(
        req.body.toString('utf8'),
        req.header('PayRouters-Signature'),
        process.env.PAYROUTERS_WEBHOOK_SECRET!,
      );
      // Dedupe on req.header('PayRouters-Event-Id'), then apply event.data.payment.
      res.sendStatus(200);
    } catch (error) {
      if (error instanceof WebhookSignatureError) {
        res.sendStatus(401);
        return;
      }
      throw error;
    }
  },
);
```

Headers from a Node request can also be passed straight to `getSignatureHeader(req.headers)`, which matches the header case-insensitively.

### Cloudflare Workers `fetch` handler

```ts
import { constructEvent, WebhookSignatureError } from '@payrouters/sdk';

interface Env {
  PAYROUTERS_WEBHOOK_SECRET: string;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const rawBody = await request.text();
    try {
      const event = await constructEvent(
        rawBody,
        request.headers.get('PayRouters-Signature'),
        env.PAYROUTERS_WEBHOOK_SECRET,
      );
      // Dedupe on request.headers.get('PayRouters-Event-Id'), then apply event.data.payment.
      return new Response('ok');
    } catch (error) {
      if (error instanceof WebhookSignatureError) {
        return new Response('invalid signature', { status: 401 });
      }
      throw error;
    }
  },
};
```

## Module formats

The package ships both formats, selected through `exports`:

- `import` → `dist/esm` (ESM, with `.d.ts`)
- `require` → `dist/cjs` (CommonJS, with `.d.ts`)

Both builds expose the same API. TypeScript projects get correct types under `module: node16`/`nodenext` (CJS or ESM mode) and under `bundler` resolution.

## Development

```sh
pnpm install
pnpm build      # tsc → dist/ with .d.ts and source maps
pnpm typecheck  # tests included
pnpm test       # vitest, fully offline (fake fetch, fixed clock)
```
