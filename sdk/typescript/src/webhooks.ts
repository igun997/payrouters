import { WebhookSignatureError } from './errors.js';
import { hmacSha256Hex, timingSafeEqualString } from './crypto.js';
import type { PayRouterEvent } from './types.js';

/** Header carrying `t=<unix_seconds>,v1=<hex>[,v1=<hex>…]`. */
export const SIGNATURE_HEADER = 'PayRouters-Signature';
/** Header carrying the event id (`evt_…`); dedupe on it. */
export const EVENT_ID_HEADER = 'PayRouters-Event-Id';
export const DEFAULT_TOLERANCE_SECONDS = 300;

/** Anything with a Fetch-style `get`, or a Node `req.headers` object (values may be arrays). */
export type HeadersLike =
  | { get(name: string): string | null }
  | Record<string, string | readonly string[] | undefined>;

export interface VerifyWebhookOptions {
  /** The exact raw request body as received. Do not re-serialize parsed JSON. */
  payload: string;
  /** The `PayRouters-Signature` header value. */
  header: string | null | undefined;
  /** The product's webhook secret (`whsec_…`), used verbatim as the HMAC key. */
  secret: string;
  /** Allowed clock skew in seconds. Default 300. */
  toleranceSeconds?: number | undefined;
  /** Returns the current time in unix seconds. Default `Date.now() / 1000`. */
  now?: (() => number) | undefined;
}

export interface ConstructEventOptions {
  toleranceSeconds?: number | undefined;
  now?: (() => number) | undefined;
}

function hasGet(headers: HeadersLike): headers is { get(name: string): string | null } {
  return 'get' in headers && typeof headers.get === 'function';
}

/**
 * Reads `PayRouters-Signature` from any headers shape, case-insensitively.
 * Duplicate header values are joined with `,`, which the parser accepts.
 */
export function getSignatureHeader(headers: HeadersLike): string | undefined {
  if (hasGet(headers)) {
    return headers.get(SIGNATURE_HEADER) ?? undefined;
  }
  const wanted = SIGNATURE_HEADER.toLowerCase();
  for (const key of Object.keys(headers)) {
    if (key.toLowerCase() !== wanted) continue;
    const value = headers[key];
    if (value === undefined) continue;
    return typeof value === 'string' ? value : value.join(',');
  }
  return undefined;
}

interface ParsedSignatureHeader {
  /** Timestamp exactly as sent; the HMAC is computed over this text. */
  timestampText: string;
  timestamp: number;
  /** Lowercase-normalized `v1` digests. */
  signatures: string[];
}

const INTEGER = /^\d+$/;
const HEX = /^[0-9a-fA-F]+$/;

function parseSignatureHeader(header: string): ParsedSignatureHeader {
  let timestampText: string | undefined;
  const signatures: string[] = [];

  for (const part of header.split(',')) {
    const eq = part.indexOf('=');
    if (eq < 0) continue;
    const key = part.slice(0, eq).trim();
    const value = part.slice(eq + 1).trim();

    if (key === 't') {
      if (timestampText !== undefined || !INTEGER.test(value)) {
        throw new WebhookSignatureError('malformed_header', 'PayRouters-Signature has an invalid t= value');
      }
      timestampText = value;
    } else if (key === 'v1') {
      if (HEX.test(value)) signatures.push(value.toLowerCase());
    }
    // Unknown schemes (e.g. a future v0) are ignored.
  }

  if (timestampText === undefined) {
    throw new WebhookSignatureError('malformed_header', 'PayRouters-Signature is missing t=');
  }
  return { timestampText, timestamp: Number(timestampText), signatures };
}

/**
 * Verifies a PayRouters webhook signature. Resolves when authentic and fresh; throws
 * `WebhookSignatureError` otherwise. Pass the raw body string exactly as received.
 *
 * Expected: `v1 = hex(HMAC_SHA256(secret, "<t>." + payload))`. Any `v1` entry may match
 * (supports key rotation). Rejects when `|now - t| > toleranceSeconds`.
 */
export async function verifyWebhookSignature(options: VerifyWebhookOptions): Promise<void> {
  const { payload, header, secret } = options;
  if (typeof payload !== 'string') throw new TypeError('payload must be the raw body string');
  if (!secret) throw new TypeError('webhook secret is required');
  if (header === undefined || header === null || header === '') {
    throw new WebhookSignatureError('missing_header', 'PayRouters-Signature header is missing');
  }

  const parsed = parseSignatureHeader(header);
  if (parsed.signatures.length === 0) {
    throw new WebhookSignatureError('no_signature', 'PayRouters-Signature has no v1= signature');
  }

  const tolerance = options.toleranceSeconds ?? DEFAULT_TOLERANCE_SECONDS;
  const now = options.now ? options.now() : Math.floor(Date.now() / 1000);
  if (Math.abs(now - parsed.timestamp) > tolerance) {
    throw new WebhookSignatureError(
      'timestamp_out_of_tolerance',
      `Webhook timestamp is outside the ${tolerance}s tolerance window`,
    );
  }

  const expected = await hmacSha256Hex(secret, `${parsed.timestampText}.${payload}`);
  // No early exit: every candidate is compared so timing does not reveal which entry matched.
  let matched = false;
  for (const candidate of parsed.signatures) {
    if (timingSafeEqualString(expected, candidate)) matched = true;
  }
  if (!matched) {
    throw new WebhookSignatureError('signature_mismatch', 'Webhook signature does not match');
  }
}

/**
 * Verifies the signature, then parses the body into a typed event.
 * The signature covers the raw body, so verify before trusting any field.
 */
export async function constructEvent(
  rawBody: string,
  header: string | null | undefined,
  secret: string,
  options: ConstructEventOptions = {},
): Promise<PayRouterEvent> {
  await verifyWebhookSignature({
    payload: rawBody,
    header,
    secret,
    toleranceSeconds: options.toleranceSeconds,
    now: options.now,
  });
  // Unchecked cast by design: the HMAC above authenticates this body as one the server
  // produced, so its documented shape is trusted rather than re-validated field by field.
  const event = JSON.parse(rawBody) as PayRouterEvent;
  return event;
}
