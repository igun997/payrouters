# Checkout and methods

- `checkout: "snap"` (default): PayRouters shows its own page (`snap_url`) built on the provider's **core API**: virtual account numbers and QRIS with a countdown and live status. Without `method` the customer picks one; with `method` the instructions are created immediately.
- `checkout: "provider"`: the provider's hosted page (Midtrans Snap, Xendit Payment Session, IDRX checkout, Mayar payment request, PayPal checkout). The URL comes back as `instructions.type = "redirect"`. Mayar and PayPal have no standard methods, so they always use their hosted page, even with `checkout: "snap"` (the Snap page then shows the hop to it) and refuse a `method`.

Standard method codes: `va_bca`, `va_bni`, `va_bri`, `va_mandiri`, `va_permata`, `qris`. Each provider supports a subset; `list_payment_methods` (this MCP) or `GET {{base_url}}/v1/methods` (product API) lists what the current mode can use, and only providers with an enabled account appear.

Snap page behaviour: the customer picks a method, sees the instructions and a countdown, and the page checks the status every few seconds. When the payment is no longer pending it redirects to `return_url` if one was given. Treat that redirect as a hint: fulfil the order from the webhook or a `GET /v1/payments/:id`.
