<script lang="ts">
	// Product API guide for the developers who integrate a product with PayRouters. The contract
	// for implementers lives in docs/api.md; this page covers what a product needs: keys, creating
	// payments, statuses, webhooks, errors and the sandbox. Examples use this dashboard's origin as
	// the base URL, since the API is served by the same binary.
	import PageHeader from '#lib/components/app/page-header.svelte';
	import CodeBlock from '#lib/components/app/code-block.svelte';
	import * as Table from '#lib/components/ui/table';
	import { resolve } from '$app/paths';

	const base = typeof window === 'undefined' ? 'https://pay.example.com' : window.location.origin;

	const sections = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'authentication', label: 'Authentication' },
		{ id: 'create-payment', label: 'Create a payment' },
		{ id: 'checkout', label: 'Checkout and methods' },
		{ id: 'status', label: 'Payment status' },
		{ id: 'webhooks', label: 'Webhooks' },
		{ id: 'snap', label: 'The Snap page' },
		{ id: 'errors', label: 'Errors and limits' },
		{ id: 'sandbox', label: 'Sandbox' },
		{ id: 'sdk', label: 'TypeScript SDK' },
		{ id: 'mcp', label: 'Connect an AI assistant' }
	];

	const fields: { name: string; type: string; note: string }[] = [
		{
			name: 'order_id',
			type: 'string',
			note: 'Required. Up to 64 characters, unique per product and mode.'
		},
		{
			name: 'amount',
			type: 'integer',
			note: 'Required. Rupiah, at least 1. For IDRX this is the IDRX to mint, at least 20000.'
		},
		{
			name: 'provider',
			type: 'string',
			note: 'midtrans, xendit, idrx or mock. Defaults to the product default, then the first enabled provider of the key mode.'
		},
		{
			name: 'checkout',
			type: 'string',
			note: 'snap (default) or provider. See Checkout and methods.'
		},
		{
			name: 'method',
			type: 'string',
			note: 'va_bca, va_bni, va_bri, va_mandiri, va_permata or qris. With snap it creates the charge now.'
		},
		{ name: 'description', type: 'string', note: 'Shown to the customer.' },
		{ name: 'customer', type: 'object', note: 'name, email and phone, all optional.' },
		{
			name: 'items',
			type: 'array',
			note: 'id, name, price and quantity. Prices must add up to amount.'
		},
		{
			name: 'metadata',
			type: 'object',
			note: 'Returned unchanged on the payment and in webhooks.'
		},
		{ name: 'expires_in', type: 'integer', note: 'Seconds. Default 86400, minimum 300.' },
		{
			name: 'return_url',
			type: 'string',
			note: 'Where the Snap page sends the customer once the payment is no longer pending.'
		},
		{
			name: 'provider_options',
			type: 'object',
			note: 'IDRX only: destination_wallet_address and chain_id. Both fall back to the provider account defaults.'
		}
	];

	const statuses: { name: string; note: string }[] = [
		{
			name: 'pending',
			note: 'Created and waiting for the customer. The only non-terminal status.'
		},
		{
			name: 'paid',
			note: 'Settled. Final: it never changes afterwards. fee_amount and net_amount are set.'
		},
		{ name: 'expired', note: 'Not paid before expires_at.' },
		{ name: 'failed', note: 'The provider rejected or failed the charge.' },
		{ name: 'cancelled', note: 'Cancelled before it was paid.' }
	];

	const errors: { status: string; code: string; note: string }[] = [
		{
			status: '400',
			code: 'invalid_request',
			note: 'A field is missing or malformed. details says which.'
		},
		{ status: '401', code: 'unauthorized', note: 'The key is missing, wrong or revoked.' },
		{ status: '404', code: 'not_found', note: 'No such payment for this key.' },
		{
			status: '409',
			code: 'order_id_conflict',
			note: 'The order_id exists with a different amount or provider, or is no longer pending.'
		},
		{
			status: '409',
			code: 'method_already_selected',
			note: 'The payment already has payment instructions.'
		},
		{
			status: '422',
			code: 'provider_unavailable',
			note: 'No enabled provider account for this mode.'
		},
		{ status: '422', code: 'method_unsupported', note: 'The provider does not offer that method.' },
		{
			status: '422',
			code: 'sandbox_unsupported',
			note: 'A sandbox key asked for IDRX, which has no sandbox.'
		},
		{
			status: '429',
			code: 'rate_limited',
			note: 'Too many failed attempts or requests. Wait Retry-After seconds.'
		},
		{
			status: '502',
			code: 'provider_error',
			note: 'The provider answered with an error. Safe to retry with the same order_id.'
		},
		{ status: '500', code: 'internal', note: 'Unexpected server error.' }
	];

	const createCurl = `curl -X POST ${base}/v1/payments \\
  -H "Authorization: Bearer prk_test_..." \\
  -H "Content-Type: application/json" \\
  -d '{
    "order_id": "INV-2026-0001",
    "amount": 150000,
    "method": "va_bca",
    "customer": { "name": "Budi", "email": "budi@example.com" },
    "return_url": "https://app.example.com/thanks"
  }'`;

	const createResponse = `{
  "id": "0192...",
  "order_id": "INV-2026-0001",
  "status": "pending",
  "livemode": false,
  "amount": 150000,
  "payable_amount": null,
  "currency": "IDR",
  "provider": "midtrans",
  "method": "va_bca",
  "checkout": "snap",
  "snap_url": "${base}/snap/Zx9...",
  "instructions": {
    "type": "virtual_account",
    "bank": "bca",
    "account_number": "1234567890"
  },
  "expires_at": "2026-11-27T10:00:00Z"
}`;

	const methodsCurl = `curl ${base}/v1/methods \\
  -H "Authorization: Bearer prk_test_..."`;

	const getCurl = `curl ${base}/v1/payments/0192... \\
  -H "Authorization: Bearer prk_test_..."

curl "${base}/v1/payments?order_id=INV-2026-0001&status=paid" \\
  -H "Authorization: Bearer prk_test_..."`;

	const eventBody = `{
  "id": "evt_0192...",
  "type": "payment.paid",
  "livemode": false,
  "created_at": "2026-11-26T10:02:09Z",
  "data": { "payment": { "id": "0192...", "order_id": "INV-2026-0001", "status": "paid" } }
}`;

	const verifyCode = `import { constructEvent, WebhookSignatureError } from '@payrouters/sdk';

// rawBody is the request body exactly as received, not re-serialized JSON.
try {
  const event = await constructEvent(
    rawBody,
    request.headers.get('PayRouters-Signature'),
    process.env.PAYROUTERS_WEBHOOK_SECRET!
  );
  // Skip it if PayRouters-Event-Id was already handled, then apply event.data.payment.
  return new Response('ok');
} catch (error) {
  if (error instanceof WebhookSignatureError) return new Response('invalid', { status: 401 });
  throw error;
}`;

	const sdkCode = `import { PayRouters } from '@payrouters/sdk';

const pr = new PayRouters({
  apiKey: process.env.PAYROUTERS_API_KEY!, // prk_test_... or prk_live_...
  baseUrl: '${base}'
});

const { payment, replayed } = await pr.payments.create({
  order_id: 'INV-2026-0001',
  amount: 150_000,
  method: 'va_bca'
});

console.log(payment.snap_url, payment.payable_amount ?? payment.amount);`;
</script>

<PageHeader
	title="API docs"
	description="Create payments, receive webhooks and handle errors with the product API."
/>

<div class="grid gap-section xl:grid-cols-[minmax(0,1fr)_200px]" data-testid="page-docs">
	<div class="flex min-w-0 flex-col gap-section">
		<section id="overview" class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card">
			<h2 class="text-card-title">Overview</h2>
			<p class="text-body text-muted-foreground">
				One API in front of Midtrans, Xendit and IDRX. Your backend creates a payment, sends the
				customer to the Snap page (or the provider page), and learns the result from a signed
				webhook. Amounts are integer rupiah and every timestamp is UTC.
			</p>
			<p class="text-body text-muted-foreground">Base URL of this deployment:</p>
			<CodeBlock code={base} caption="Base URL" testid="docs-base-url" />
		</section>

		<section
			id="authentication"
			class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card"
		>
			<h2 class="text-card-title">Authentication</h2>
			<p class="text-body text-muted-foreground">
				Send a product API key as a bearer token. A key is bound to one mode: <code
					class="text-mono-id font-mono"
				>
					prk_test_
				</code>
				keys reach sandbox provider accounts, <code class="text-mono-id font-mono">prk_live_</code>
				keys reach live ones. Create keys on the
				<a class="underline underline-offset-4" href={resolve('/(admin)/products')}>Products</a>
				page; the full key is shown once.
			</p>
			<CodeBlock code="Authorization: Bearer prk_test_..." caption="Header" />
			<p class="text-helper text-muted-foreground">
				Repeated failed attempts lock the caller out for a few minutes, even with a correct key.
			</p>
		</section>

		<section
			id="create-payment"
			class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card"
		>
			<h2 class="text-card-title">Create a payment</h2>
			<p class="text-body text-muted-foreground">
				<code class="text-mono-id font-mono">POST /v1/payments</code> returns
				<code class="text-mono-id font-mono">201</code>
				with the payment. Only <code class="text-mono-id font-mono">order_id</code> and
				<code class="text-mono-id font-mono">amount</code> are required.
			</p>
			<CodeBlock code={createCurl} caption="Request (curl)" />
			<CodeBlock code={createResponse} caption="Response 201 (shortened)" />
			<Table.Root>
				<Table.Header>
					<Table.Row class="hover:bg-transparent">
						<Table.Head>Field</Table.Head>
						<Table.Head>Type</Table.Head>
						<Table.Head>Notes</Table.Head>
					</Table.Row>
				</Table.Header>
				<Table.Body>
					{#each fields as field (field.name)}
						<Table.Row>
							<Table.Cell class="text-mono-id font-mono break-all whitespace-normal">
								{field.name}
							</Table.Cell>
							<Table.Cell class="whitespace-normal text-muted-foreground">{field.type}</Table.Cell>
							<Table.Cell class="break-words whitespace-normal">{field.note}</Table.Cell>
						</Table.Row>
					{/each}
				</Table.Body>
			</Table.Root>
			<h3 class="text-label">Idempotency</h3>
			<p class="text-body text-muted-foreground">
				Repeat the call with the same <code class="text-mono-id font-mono">order_id</code>, amount
				and provider while the payment is still pending and you get
				<code class="text-mono-id font-mono">200</code>
				with the existing payment and the header
				<code class="text-mono-id font-mono">PayRouters-Idempotent-Replay: true</code>. Any other
				duplicate is <code class="text-mono-id font-mono">409 order_id_conflict</code>. After a
				timeout, retry with the same order_id instead of inventing a new one.
			</p>
		</section>

		<section id="checkout" class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card">
			<h2 class="text-card-title">Checkout and methods</h2>
			<ul class="flex list-disc flex-col gap-2 pl-5 text-body text-muted-foreground">
				<li>
					<strong class="text-foreground">snap</strong> uses the PayRouters page on the provider's
					core API. Without a method the customer chooses one on the page; with a method the charge
					is created now and <code class="text-mono-id font-mono">instructions</code> is returned.
				</li>
				<li>
					<strong class="text-foreground">provider</strong> uses the provider's own page (Midtrans
					Snap, Xendit session, IDRX checkout). The URL comes back as
					<code class="text-mono-id font-mono">instructions.type = "redirect"</code>.
				</li>
			</ul>
			<p class="text-body text-muted-foreground">
				Instructions are tagged by type: <code class="text-mono-id font-mono">virtual_account</code>
				(bank, account number, or biller code and bill key for Mandiri),
				<code class="text-mono-id font-mono">qris</code> (QR string and image) and
				<code class="text-mono-id font-mono">redirect</code>. Show
				<code class="text-mono-id font-mono">payable_amount ?? amount</code> as the total to pay: some
				providers add their fee on top.
			</p>
			<p class="text-body text-muted-foreground">
				List what your key can use right now. Only providers with an enabled account in the key's
				mode appear.
			</p>
			<CodeBlock code={methodsCurl} caption="Request (curl)" />
		</section>

		<section id="status" class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card">
			<h2 class="text-card-title">Payment status</h2>
			<Table.Root>
				<Table.Header>
					<Table.Row class="hover:bg-transparent">
						<Table.Head>Status</Table.Head>
						<Table.Head>Meaning</Table.Head>
					</Table.Row>
				</Table.Header>
				<Table.Body>
					{#each statuses as status (status.name)}
						<Table.Row>
							<Table.Cell class="text-mono-id font-mono break-all whitespace-normal">
								{status.name}
							</Table.Cell>
							<Table.Cell class="break-words whitespace-normal">{status.note}</Table.Cell>
						</Table.Row>
					{/each}
				</Table.Body>
			</Table.Root>
			<p class="text-body text-muted-foreground">
				Prefer the webhook over polling. To read a payment, or to ask PayRouters to re-read the
				provider after a missed webhook, use:
			</p>
			<CodeBlock code={getCurl} caption="Read (curl)" />
			<p class="text-helper text-muted-foreground">
				<code class="text-mono-id font-mono">POST /v1/payments/:id/sync</code> re-reads the provider
				and applies any status change. Lists take <code class="text-mono-id font-mono">limit</code>
				(default 50, at most 200) and <code class="text-mono-id font-mono">cursor</code>, and return
				<code class="text-mono-id font-mono">next_cursor</code>.
			</p>
		</section>

		<section id="webhooks" class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card">
			<h2 class="text-card-title">Webhooks</h2>
			<p class="text-body text-muted-foreground">
				When a payment reaches a terminal status PayRouters sends <code
					class="text-mono-id font-mono"
				>
					POST
				</code>
				to the product's webhook URL with one of
				<code class="text-mono-id font-mono">payment.paid</code>,
				<code class="text-mono-id font-mono">payment.expired</code>,
				<code class="text-mono-id font-mono">payment.failed</code> or
				<code class="text-mono-id font-mono">payment.cancelled</code>. The webhook secret (<code
					class="text-mono-id font-mono"
				>
					whsec_...
				</code>) is shown when you create the product or rotate its secret.
			</p>
			<CodeBlock code={eventBody} caption="Body (shortened)" />
			<p class="text-body text-muted-foreground">
				Each request carries <code class="text-mono-id font-mono">PayRouters-Signature</code>
				(<code class="text-mono-id font-mono">t=&lt;unix seconds&gt;,v1=&lt;hex&gt;</code>) and
				<code class="text-mono-id font-mono">PayRouters-Event-Id</code>. The signature is
				<code class="text-mono-id font-mono">
					HMAC_SHA256(secret, "&lt;t&gt;.&lt;raw body&gt;")
				</code>
				in hex. Verify it against the raw body, and reject a timestamp more than 300 seconds from now.
			</p>
			<CodeBlock code={verifyCode} caption="Verify with the SDK (TypeScript)" />
			<ul class="flex list-disc flex-col gap-2 pl-5 text-body text-muted-foreground">
				<li>Answer with any <code class="text-mono-id font-mono">2xx</code> within 10 seconds.</li>
				<li>
					Failures are retried after 10 s, 1 m, 5 m, 30 m, 2 h, 6 h, 12 h and 24 h, then marked
					failed. Retry a failed delivery from
					<a class="underline underline-offset-4" href={resolve('/(admin)/webhooks')}>Webhooks</a>.
				</li>
				<li>
					Delivery is at least once: ignore an event whose <code class="text-mono-id font-mono">
						PayRouters-Event-Id
					</code> you already handled.
				</li>
			</ul>
		</section>

		<section id="snap" class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card">
			<h2 class="text-card-title">The Snap page</h2>
			<p class="text-body text-muted-foreground">
				<code class="text-mono-id font-mono">snap_url</code> opens a hosted page where the customer
				picks a method, sees the instructions and the countdown, and watches the status. The page
				checks the status every few seconds and, when the payment is no longer pending, sends the
				customer to
				<code class="text-mono-id font-mono">return_url</code> if you set one. Treat the redirect as a
				hint and trust the webhook or a GET before you fulfil an order.
			</p>
		</section>

		<section id="errors" class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card">
			<h2 class="text-card-title">Errors and limits</h2>
			<CodeBlock
				code={`{ "error": { "code": "invalid_request", "message": "amount must be at least 1", "details": null } }`}
				caption="Error body"
			/>
			<Table.Root>
				<Table.Header>
					<Table.Row class="hover:bg-transparent">
						<Table.Head>HTTP</Table.Head>
						<Table.Head>code</Table.Head>
						<Table.Head>When</Table.Head>
					</Table.Row>
				</Table.Header>
				<Table.Body>
					{#each errors as error (error.code)}
						<Table.Row>
							<Table.Cell class="tabular">{error.status}</Table.Cell>
							<Table.Cell class="text-mono-id font-mono break-all whitespace-normal">
								{error.code}
							</Table.Cell>
							<Table.Cell class="break-words whitespace-normal">{error.note}</Table.Cell>
						</Table.Row>
					{/each}
				</Table.Body>
			</Table.Root>
			<p class="text-helper text-muted-foreground">
				A 429 carries <code class="text-mono-id font-mono">Retry-After</code> in seconds. Branch on
				<code class="text-mono-id font-mono">code</code>, not on the message text.
			</p>
		</section>

		<section id="sandbox" class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card">
			<h2 class="text-card-title">Sandbox</h2>
			<ul class="flex list-disc flex-col gap-2 pl-5 text-body text-muted-foreground">
				<li>
					Sandbox keys only reach sandbox provider accounts: Midtrans sandbox, Xendit test keys and
					the built-in <code class="text-mono-id font-mono">mock</code> provider.
				</li>
				<li>
					With <code class="text-mono-id font-mono">mock</code> the Snap page shows a simulator that
					marks the payment paid, expired or failed, through the same pipeline as a real provider.
					The
					<a class="underline underline-offset-4" href={resolve('/(admin)/playground')}>
						Playground
					</a>
					creates payments without an API key.
				</li>
				<li>
					IDRX has no sandbox. A sandbox key asking for it gets <code
						class="text-mono-id font-mono"
					>
						422 sandbox_unsupported
					</code>.
				</li>
				<li>
					Sandbox and live data never mix in the dashboard stats. Use the mode switch in the top
					bar.
				</li>
			</ul>
		</section>

		<section id="sdk" class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card">
			<h2 class="text-card-title">TypeScript SDK</h2>
			<p class="text-body text-muted-foreground">
				<code class="text-mono-id font-mono">@payrouters/sdk</code> wraps the product API and the
				webhook check. It has no runtime dependencies and runs on Node 18+, Bun, Deno, Workers and
				browsers. <code class="text-mono-id font-mono">payments.create</code> is never retried automatically;
				read calls are retried on network errors, 5xx and 429.
			</p>
			<CodeBlock code="pnpm add @payrouters/sdk" caption="Install" />
			<CodeBlock code={sdkCode} caption="Create a payment (TypeScript)" />
		</section>

		<section id="mcp" class="flex scroll-mt-4 flex-col gap-3 rounded-lg border bg-card p-card">
			<h2 class="text-card-title">Connect an AI assistant</h2>
			<p class="text-body text-muted-foreground">
				PayRouters is an MCP server with OAuth. An assistant such as Claude Code or Cursor connects
				without anyone pasting a token: it opens your browser, the signed-in dashboard session
				approves it, and it can then read your configuration, create a product and a sandbox key,
				generate code with the right base URL and test your webhook endpoint, instead of guessing
				field names from these docs.
			</p>
			<CodeBlock
				code={`claude mcp add --transport http payrouters ${base}/mcp`}
				caption="Claude Code"
			/>
			<p class="text-helper text-muted-foreground">
				Live mode is off unless you tick it on the consent screen. Manage connections under
				<a class="underline underline-offset-4" href={resolve('/(admin)/integrations')}>
					Integrations
				</a>.
			</p>
		</section>
	</div>

	<nav aria-label="On this page" class="hidden xl:block" data-testid="docs-toc">
		<ul class="sticky top-4 flex flex-col gap-1 border-l pl-4">
			{#each sections as section (section.id)}
				<li>
					<a
						href={`#${section.id}`}
						class="inline-flex min-h-touch items-center text-helper text-muted-foreground hover:text-foreground xl:min-h-8"
					>
						{section.label}
					</a>
				</li>
			{/each}
		</ul>
	</nav>
</div>
