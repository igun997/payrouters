/** Error codes documented in the API contract; the server may add more. */
export type PayRoutersErrorCode =
  | 'invalid_request'
  | 'unauthorized'
  | 'not_found'
  | 'conflict'
  | 'order_id_conflict'
  | 'method_already_selected'
  | 'unprocessable'
  | 'provider_unavailable'
  | 'method_unsupported'
  | 'sandbox_unsupported'
  | 'provider_error'
  | 'internal'
  | 'http_error'
  | 'invalid_response'
  | (string & {});

export interface PayRoutersErrorInit {
  status: number;
  code: string;
  message: string;
  details?: unknown;
  requestId?: string | undefined;
}

/** A non-2xx response from the PayRouters API, built from the `{ error: { code, message, details } }` envelope. */
export class PayRoutersError extends Error {
  override name = 'PayRoutersError';
  readonly status: number;
  readonly code: PayRoutersErrorCode;
  readonly details: unknown;
  /** Value of the `x-request-id` response header, when present. */
  readonly requestId: string | undefined;

  constructor(init: PayRoutersErrorInit) {
    super(init.message);
    this.status = init.status;
    this.code = init.code;
    this.details = init.details ?? null;
    this.requestId = init.requestId;
  }
}

/** The request never produced an HTTP response: timeout, DNS/TCP/TLS failure, or a body read failure. */
export class PayRoutersNetworkError extends Error {
  override name = 'PayRoutersNetworkError';
  readonly kind: 'timeout' | 'transport';

  constructor(init: { kind: 'timeout' | 'transport'; message: string; cause?: unknown }) {
    super(init.message, { cause: init.cause });
    this.kind = init.kind;
  }
}

export type WebhookSignatureReason =
  /** No `PayRouters-Signature` header was supplied. */
  | 'missing_header'
  /** Header lacks a valid integer `t=` or has a duplicate/invalid `t=`. */
  | 'malformed_header'
  /** Header has no `v1=` entry. */
  | 'no_signature'
  /** `|now - t|` exceeds the tolerance window. */
  | 'timestamp_out_of_tolerance'
  /** None of the `v1=` entries match the expected HMAC. */
  | 'signature_mismatch';

/** Thrown by `verifyWebhookSignature` and `constructEvent` when the webhook is not authentic or is stale. */
export class WebhookSignatureError extends Error {
  override name = 'WebhookSignatureError';
  readonly reason: WebhookSignatureReason;

  constructor(reason: WebhookSignatureReason, message: string) {
    super(message);
    this.reason = reason;
  }
}
