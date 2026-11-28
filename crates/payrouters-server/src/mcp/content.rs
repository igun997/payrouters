//! What the MCP server tells an assistant besides tool results: the documentation topics
//! (resources), ready-to-paste code for the common integration tasks, and the prompt texts.

use crate::state::AppState;

// --- Documentation topics -----------------------------------------------------------------------

pub struct Topic {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    body: &'static str,
}

pub const TOPICS: [Topic; 10] = [
    Topic {
        id: "overview",
        title: "PayRouters overview",
        summary: "What PayRouters is, base URL, modes and the shortest integration",
        body: include_str!("docs/overview.md"),
    },
    Topic {
        id: "authentication",
        title: "Authentication",
        summary: "Product API keys, test versus live, key handling",
        body: include_str!("docs/authentication.md"),
    },
    Topic {
        id: "create-payment",
        title: "Create a payment",
        summary: "POST /v1/payments fields, response, idempotency, reading payments",
        body: include_str!("docs/create-payment.md"),
    },
    Topic {
        id: "checkout-and-methods",
        title: "Checkout and methods",
        summary: "Snap page versus provider page, method codes, return_url",
        body: include_str!("docs/checkout-and-methods.md"),
    },
    Topic {
        id: "payment-status",
        title: "Payment status",
        summary: "pending, paid, expired, failed, cancelled and how they settle",
        body: include_str!("docs/payment-status.md"),
    },
    Topic {
        id: "webhooks",
        title: "Webhooks",
        summary: "Events, signature verification, retries, idempotent handling",
        body: include_str!("docs/webhooks.md"),
    },
    Topic {
        id: "errors-and-limits",
        title: "Errors and limits",
        summary: "Error envelope, every error code, pagination, rate limits",
        body: include_str!("docs/errors-and-limits.md"),
    },
    Topic {
        id: "sandbox",
        title: "Sandbox",
        summary: "Test keys, the mock provider and simulator, going live",
        body: include_str!("docs/sandbox.md"),
    },
    Topic {
        id: "testing",
        title: "Testing an integration in sandbox",
        summary: "Prove the application works end to end with the self-test tools",
        body: include_str!("docs/testing.md"),
    },
    Topic {
        id: "providers",
        title: "Provider accounts",
        summary: "Midtrans, Xendit, IDRX, Mayar and PayPal credentials and where to paste the webhook URL",
        body: include_str!("docs/providers.md"),
    },
];

pub fn topic(id: &str) -> Option<&'static Topic> {
    TOPICS.iter().find(|t| t.id == id)
}

pub fn topic_ids() -> Vec<&'static str> {
    TOPICS.iter().map(|t| t.id).collect()
}

pub fn uri(id: &str) -> String {
    format!("payrouters://docs/{id}")
}

pub fn topic_by_uri(uri: &str) -> Option<&'static Topic> {
    uri.strip_prefix("payrouters://docs/").and_then(topic)
}

impl Topic {
    pub fn render(&self, state: &AppState) -> String {
        fill(self.body, state)
    }
}

fn fill(text: &str, state: &AppState) -> String {
    text.replace("{{base_url}}", &state.config.public_base_url)
        .replace("{{mcp_url}}", &state.mcp_url())
}

// --- Code snippets ------------------------------------------------------------------------------

pub const TASKS: [&str; 3] = ["create_payment", "verify_webhook", "get_payment"];
pub const LANGUAGES: [&str; 4] = ["curl", "typescript", "python", "php"];

pub fn snippet(task: &str, language: &str, base_url: &str) -> Option<String> {
    let code = match (task, language) {
        ("create_payment", "curl") => CURL_CREATE,
        ("create_payment", "typescript") => TS_CREATE,
        ("create_payment", "python") => PY_CREATE,
        ("create_payment", "php") => PHP_CREATE,
        ("verify_webhook", "curl") => CURL_VERIFY,
        ("verify_webhook", "typescript") => TS_VERIFY,
        ("verify_webhook", "python") => PY_VERIFY,
        ("verify_webhook", "php") => PHP_VERIFY,
        ("get_payment", "curl") => CURL_GET,
        ("get_payment", "typescript") => TS_GET,
        ("get_payment", "python") => PY_GET,
        ("get_payment", "php") => PHP_GET,
        _ => return None,
    };
    Some(code.replace("{{base_url}}", base_url))
}

const CURL_CREATE: &str = r##"curl -X POST {{base_url}}/v1/payments \
  -H "Authorization: Bearer $PAYROUTERS_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "order_id": "INV-2026-0001",
    "amount": 150000,
    "method": "va_bca",
    "description": "Order #1",
    "customer": { "name": "Budi", "email": "budi@example.com" },
    "return_url": "https://app.example.com/thanks"
  }'
# 201 -> the payment. Send the customer to "snap_url", or show "instructions" yourself.
# Repeating the call with the same order_id and amount returns the same payment (200)."##;

const CURL_GET: &str = r##"curl {{base_url}}/v1/payments/PAYMENT_ID \
  -H "Authorization: Bearer $PAYROUTERS_API_KEY"

curl "{{base_url}}/v1/payments?order_id=INV-2026-0001" \
  -H "Authorization: Bearer $PAYROUTERS_API_KEY"

# Re-read the provider (use it when a webhook was missed):
curl -X POST {{base_url}}/v1/payments/PAYMENT_ID/sync \
  -H "Authorization: Bearer $PAYROUTERS_API_KEY""##;

const CURL_VERIFY: &str = r##"# Manual check of one delivery. BODY is the raw request body, HEADER the PayRouters-Signature header.
T=$(printf '%s' "$HEADER" | sed -n 's/.*t=\([0-9]*\).*/\1/p')
EXPECTED=$(printf '%s.%s' "$T" "$BODY" | openssl dgst -sha256 -hmac "$PAYROUTERS_WEBHOOK_SECRET" | sed 's/^.* //')
printf '%s' "$HEADER" | grep -q "v1=$EXPECTED" && echo valid || echo invalid
# Also reject when |now - T| > 300 seconds."##;

const TS_CREATE: &str = r##"// Node 18+, Bun, Deno or a Worker: only fetch is needed.
const baseUrl = process.env.PAYROUTERS_BASE_URL ?? '{{base_url}}';

export async function createPayment(orderId: string, amount: number) {
  const res = await fetch(`${baseUrl}/v1/payments`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${process.env.PAYROUTERS_API_KEY}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      order_id: orderId,
      amount,
      method: 'va_bca',
      return_url: 'https://app.example.com/thanks'
    })
  });
  const body = await res.json();
  if (!res.ok) throw new Error(`PayRouters ${res.status} ${body.error?.code}: ${body.error?.message}`);
  // Send the customer to body.snap_url, or render body.instructions yourself.
  // Show body.payable_amount ?? body.amount as the total to pay.
  return body as {
    id: string;
    status: string;
    snap_url: string;
    amount: number;
    payable_amount: number | null;
    instructions: unknown;
  };
}"##;

const TS_GET: &str = r##"const baseUrl = process.env.PAYROUTERS_BASE_URL ?? '{{base_url}}';
const headers = { Authorization: `Bearer ${process.env.PAYROUTERS_API_KEY}` };

export async function getPayment(id: string) {
  const res = await fetch(`${baseUrl}/v1/payments/${id}`, { headers });
  if (!res.ok) throw new Error(`PayRouters ${res.status}`);
  return res.json();
}

export async function findByOrderId(orderId: string) {
  const res = await fetch(`${baseUrl}/v1/payments?order_id=${encodeURIComponent(orderId)}`, { headers });
  const page = await res.json();
  return page.data[0] ?? null;
}

// Re-read the provider when a webhook was missed.
export async function syncPayment(id: string) {
  const res = await fetch(`${baseUrl}/v1/payments/${id}/sync`, { method: 'POST', headers });
  return res.json();
}"##;

const TS_VERIFY: &str = r##"import { createHmac, timingSafeEqual } from 'node:crypto';

/** rawBody must be the request body exactly as received (not re-serialized JSON). */
export function verifyPayRoutersWebhook(
  rawBody: string,
  header: string | null | undefined,
  secret: string,
  toleranceSeconds = 300
): boolean {
  if (!header) return false;
  const entries = header.split(',').map((part) => part.trim().split('='));
  const t = entries.find(([key]) => key === 't')?.[1];
  const signatures = entries.filter(([key]) => key === 'v1').map(([, value]) => value);
  if (!t || signatures.length === 0) return false;
  if (Math.abs(Date.now() / 1000 - Number(t)) > toleranceSeconds) return false;
  const expected = createHmac('sha256', secret).update(`${t}.${rawBody}`).digest('hex');
  return signatures.some(
    (s) => s.length === expected.length && timingSafeEqual(Buffer.from(s), Buffer.from(expected))
  );
}

// Express: app.post('/webhooks/payrouters', express.raw({ type: 'application/json' }), (req, res) => {
//   const ok = verifyPayRoutersWebhook(req.body.toString('utf8'), req.header('PayRouters-Signature'),
//     process.env.PAYROUTERS_WEBHOOK_SECRET!);
//   if (!ok) return res.sendStatus(401);
//   const event = JSON.parse(req.body.toString('utf8'));       // event.type, event.data.payment
//   // Skip when req.header('PayRouters-Event-Id') was already handled, then update your order.
//   res.sendStatus(200);                                        // any 2xx within 10 seconds
// });"##;

const PY_CREATE: &str = r##"import os
import requests

BASE_URL = os.environ.get("PAYROUTERS_BASE_URL", "{{base_url}}")


def create_payment(order_id: str, amount: int) -> dict:
    res = requests.post(
        f"{BASE_URL}/v1/payments",
        headers={"Authorization": f"Bearer {os.environ['PAYROUTERS_API_KEY']}"},
        json={
            "order_id": order_id,
            "amount": amount,
            "method": "va_bca",
            "return_url": "https://app.example.com/thanks",
        },
        timeout=15,
    )
    body = res.json()
    if not res.ok:
        error = body.get("error", {})
        raise RuntimeError(f"PayRouters {res.status_code} {error.get('code')}: {error.get('message')}")
    # Send the customer to body["snap_url"], or show body["instructions"] yourself.
    # Show (body["payable_amount"] or body["amount"]) as the total to pay.
    return body"##;

const PY_GET: &str = r##"import os
import requests

BASE_URL = os.environ.get("PAYROUTERS_BASE_URL", "{{base_url}}")
HEADERS = {"Authorization": f"Bearer {os.environ['PAYROUTERS_API_KEY']}"}


def get_payment(payment_id: str) -> dict:
    res = requests.get(f"{BASE_URL}/v1/payments/{payment_id}", headers=HEADERS, timeout=15)
    res.raise_for_status()
    return res.json()


def find_by_order_id(order_id: str):
    res = requests.get(f"{BASE_URL}/v1/payments", params={"order_id": order_id}, headers=HEADERS, timeout=15)
    res.raise_for_status()
    data = res.json()["data"]
    return data[0] if data else None


def sync_payment(payment_id: str) -> dict:
    # Re-read the provider when a webhook was missed.
    res = requests.post(f"{BASE_URL}/v1/payments/{payment_id}/sync", headers=HEADERS, timeout=30)
    res.raise_for_status()
    return res.json()"##;

const PY_VERIFY: &str = r##"import hashlib
import hmac
import time


def verify_payrouters_webhook(raw_body: bytes, header: str, secret: str, tolerance: int = 300) -> bool:
    """raw_body is the request body exactly as received (not re-serialized JSON)."""
    parts = [p.strip().split("=", 1) for p in (header or "").split(",") if "=" in p]
    t = next((value for key, value in parts if key == "t"), None)
    signatures = [value for key, value in parts if key == "v1"]
    if t is None or not signatures:
        return False
    try:
        timestamp = int(t)
    except ValueError:
        return False
    if abs(time.time() - timestamp) > tolerance:
        return False
    expected = hmac.new(secret.encode(), f"{t}.".encode() + raw_body, hashlib.sha256).hexdigest()
    return any(hmac.compare_digest(expected, s) for s in signatures)


# Flask: verify_payrouters_webhook(request.get_data(), request.headers.get("PayRouters-Signature", ""), SECRET)
# Answer 2xx within 10 seconds; skip events whose PayRouters-Event-Id you already handled."##;

const PHP_CREATE: &str = r##"<?php
$baseUrl = getenv('PAYROUTERS_BASE_URL') ?: '{{base_url}}';

function create_payment(string $baseUrl, string $orderId, int $amount): array {
    $ch = curl_init("$baseUrl/v1/payments");
    curl_setopt_array($ch, [
        CURLOPT_POST => true,
        CURLOPT_RETURNTRANSFER => true,
        CURLOPT_TIMEOUT => 15,
        CURLOPT_HTTPHEADER => [
            'Authorization: Bearer ' . getenv('PAYROUTERS_API_KEY'),
            'Content-Type: application/json',
        ],
        CURLOPT_POSTFIELDS => json_encode([
            'order_id' => $orderId,
            'amount' => $amount,
            'method' => 'va_bca',
            'return_url' => 'https://app.example.com/thanks',
        ]),
    ]);
    $body = json_decode((string) curl_exec($ch), true);
    $status = curl_getinfo($ch, CURLINFO_HTTP_CODE);
    curl_close($ch);
    if ($status >= 400) {
        throw new RuntimeException("PayRouters $status " . ($body['error']['code'] ?? '') . ': ' . ($body['error']['message'] ?? ''));
    }
    // Send the customer to $body['snap_url'], or show $body['instructions'] yourself.
    return $body;
}"##;

const PHP_GET: &str = r##"<?php
$baseUrl = getenv('PAYROUTERS_BASE_URL') ?: '{{base_url}}';

function payrouters_request(string $method, string $url): array {
    $ch = curl_init($url);
    curl_setopt_array($ch, [
        CURLOPT_CUSTOMREQUEST => $method,
        CURLOPT_RETURNTRANSFER => true,
        CURLOPT_TIMEOUT => 30,
        CURLOPT_HTTPHEADER => ['Authorization: Bearer ' . getenv('PAYROUTERS_API_KEY')],
    ]);
    $body = json_decode((string) curl_exec($ch), true);
    curl_close($ch);
    return $body ?? [];
}

$payment = payrouters_request('GET', "$baseUrl/v1/payments/PAYMENT_ID");
$byOrder = payrouters_request('GET', "$baseUrl/v1/payments?order_id=" . urlencode('INV-2026-0001'));
// Re-read the provider when a webhook was missed:
$synced = payrouters_request('POST', "$baseUrl/v1/payments/PAYMENT_ID/sync");"##;

const PHP_VERIFY: &str = r##"<?php
// $rawBody must be the request body exactly as received: file_get_contents('php://input').
function verify_payrouters_webhook(string $rawBody, ?string $header, string $secret, int $tolerance = 300): bool {
    if (!$header) return false;
    $t = null;
    $signatures = [];
    foreach (explode(',', $header) as $part) {
        [$key, $value] = array_pad(explode('=', trim($part), 2), 2, '');
        if ($key === 't') $t = $value;
        if ($key === 'v1') $signatures[] = $value;
    }
    if ($t === null || !ctype_digit($t) || !$signatures) return false;
    if (abs(time() - (int) $t) > $tolerance) return false;
    $expected = hash_hmac('sha256', $t . '.' . $rawBody, $secret);
    foreach ($signatures as $signature) {
        if (hash_equals($expected, $signature)) return true;
    }
    return false;
}

// $ok = verify_payrouters_webhook(file_get_contents('php://input'), $_SERVER['HTTP_PAYROUTERS_SIGNATURE'] ?? null, getenv('PAYROUTERS_WEBHOOK_SECRET'));
// if (!$ok) { http_response_code(401); exit; }
// Skip events whose PayRouters-Event-Id (HTTP_PAYROUTERS_EVENT_ID) you already handled, then answer 2xx within 10 seconds."##;

// --- Prompts ------------------------------------------------------------------------------------

pub const SERVER_INSTRUCTIONS: &str = "PayRouters routes IDR cash-in payments to Midtrans, Xendit, IDRX, Mayar and PayPal behind one API. Use these tools instead of guessing values. Start with get_integration_status: it lists the provider accounts, products, keys and the next steps. get_docs (or the payrouters://docs/* resources) explains the API; generate_code returns ready code with this deployment's base URL; run_integration_selftest, wait_for_payment, create_test_payment and simulate_payment prove an integration end to end in sandbox (docs topic testing). Secrets (API keys, webhook secrets) are returned once by the tool that creates them: tell the user to store them in their backend's environment, never in source code. Live mode needs the live permission; without it everything works on sandbox data only.";

pub const PROMPTS: [(&str, &str); 4] = [
    (
        "integrate_payrouters",
        "Integrate PayRouters payments into the user's application step by step, using the tools.",
    ),
    (
        "self_test_integration",
        "Test the user's running application end to end in sandbox: its checkout, its webhook endpoint and its handling of every outcome.",
    ),
    (
        "debug_payment",
        "Find out why a payment is not paid or its webhook did not arrive.",
    ),
    (
        "go_live_checklist",
        "Check what is left before taking real payments.",
    ),
];

pub fn integrate_prompt(language: Option<&str>, framework: Option<&str>) -> String {
    let stack = match (language, framework) {
        (Some(l), Some(f)) => format!(" ({l}, {f})"),
        (Some(l), None) => format!(" ({l})"),
        (None, Some(f)) => format!(" ({f})"),
        (None, None) => String::new(),
    };
    format!(
        "I want to accept payments with PayRouters in my application{stack}. Use the PayRouters MCP tools instead of guessing any value:\n\
1. Call get_integration_status and follow its next_steps. Ask me for anything only I know (the product name, the public URL of my webhook endpoint).\n\
2. If there is no product yet, call create_product with my webhook URL. Create a sandbox key with create_api_key. Tell me to store PAYROUTERS_API_KEY and PAYROUTERS_WEBHOOK_SECRET in my backend's environment (use get_env_template for the exact variable block), never in source code.\n\
3. Read get_docs topics create-payment and webhooks, then look at my project and write the integration code with generate_code as the reference: create a payment when an order is placed, redirect the customer to snap_url, and a webhook endpoint that verifies the signature on the raw body and marks the order paid on payment.paid (idempotent on PayRouters-Event-Id).\n\
4. Prove it in sandbox: run create_test_payment and simulate_payment, then test_webhook_integration against my endpoint, and fix whatever it reports.\n\
5. Finish with a short list of what I must do before going live."
    )
}

pub fn self_test_prompt(product: Option<&str>) -> String {
    let which = product
        .map(|p| format!("the product \"{p}\""))
        .unwrap_or_else(|| {
            "my product (ask me which one if get_integration_status lists several)".to_string()
        });
    format!(
        "Test my application's PayRouters integration end to end in sandbox for {which}. Use the tools, never the live mode:\n\
1. Call get_integration_status. The product needs a sandbox key and a webhook_url that PayRouters can reach (a public https URL; for a local server start a tunnel such as cloudflared or ngrok and set it with update_product).\n\
2. Run run_integration_selftest. It delivers signed paid, failed and expired events to my endpoint and probes a wrong signature, a missing signature, a stale timestamp and a repeated event. Fix every fail in my code (use generate_code task verify_webhook as the reference) and warn if I should, then run it again until it passes.\n\
3. Test the checkout the way a customer does: start my application with the sandbox key (get_env_template) and provider mock, trigger a purchase in it (run its flow, or call its own create-order route), then wait_for_payment for the payment it created. Check the amount, order_id, method and return_url against what I expect.\n\
4. simulate_payment paid on that payment. The result says what my endpoint answered. Then confirm in my application that the order is marked paid exactly once (read its database or logs), and repeat with failed and expired to see the order is not fulfilled.\n\
5. Report what passed, what you fixed, and what I must still check by hand."
    )
}

pub fn debug_prompt(payment_id: Option<&str>, order_id: Option<&str>) -> String {
    let which = match (payment_id, order_id) {
        (Some(id), _) => format!("payment {id}"),
        (None, Some(order)) => format!("the payment for order {order}"),
        (None, None) => "my payment (ask me for the payment id or order id)".to_string(),
    };
    format!(
        "Help me find out what is wrong with {which}. Call get_payment first (it returns the status, the provider events and the webhook deliveries), then explain: is it still pending and why (provider, method, expires_at), did the provider notify PayRouters (list_inbound_webhook_events), and did my product's webhook endpoint receive the result (list_webhook_deliveries: status, attempts, last status code and error). If a webhook failed, say what the response means and use retry_webhook_delivery after the endpoint is fixed. If the status looks stale, call sync_payment."
    )
}

pub const GO_LIVE_PROMPT: &str = "Walk me through going live with PayRouters. Call get_integration_status and check, for the product I name: a live provider account is enabled (list_provider_accounts), a live API key exists (create_api_key with mode live needs the live permission, tell me if it is missing), the webhook_url is an https URL on my production backend, and the webhook secret in my production environment is the product's current one. Then list what I should test with a real small payment, and what to watch afterwards (list_payments, list_webhook_deliveries, get_stats_summary for mode live).";
