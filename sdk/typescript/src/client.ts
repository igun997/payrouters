import { MethodsResource, PaymentsResource } from './resources.js';
import { Transport } from './transport.js';

export interface PayRoutersOptions {
  /** Product API key: `prk_test_…` (sandbox) or `prk_live_…` (live). */
  apiKey: string;
  /** Origin of the PayRouters deployment, e.g. `https://pay.example.com`. */
  baseUrl: string;
  /** Per-attempt timeout covering headers and body. Default 15000. */
  timeoutMs?: number | undefined;
  /** Custom `fetch` implementation. Default: the global `fetch`. */
  fetch?: typeof fetch | undefined;
  /** Extra attempts for GET requests on network errors, 5xx and 429. Default 2. */
  maxRetries?: number | undefined;
  /** Delay before the first GET retry; doubles each retry. Default 250. */
  retryDelayMs?: number | undefined;
}

const DEFAULT_TIMEOUT_MS = 15_000;
const DEFAULT_MAX_RETRIES = 2;
const DEFAULT_RETRY_DELAY_MS = 250;

/** Client for the PayRouters product API. Webhook helpers are standalone exports. */
export class PayRouters {
  readonly payments: PaymentsResource;
  readonly methods: MethodsResource;

  constructor(options: PayRoutersOptions) {
    if (!options.apiKey) throw new TypeError('apiKey is required');
    // URL parsing rejects malformed origins early; href normalizes a bare origin's trailing slash.
    const baseUrl = new URL(options.baseUrl).href.replace(/\/+$/, '');

    const transport = new Transport({
      apiKey: options.apiKey,
      baseUrl,
      timeoutMs: options.timeoutMs ?? DEFAULT_TIMEOUT_MS,
      fetch: options.fetch,
      maxRetries: options.maxRetries ?? DEFAULT_MAX_RETRIES,
      retryDelayMs: options.retryDelayMs ?? DEFAULT_RETRY_DELAY_MS,
    });
    this.payments = new PaymentsResource(transport);
    this.methods = new MethodsResource(transport);
  }
}
