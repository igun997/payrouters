import { PayRoutersError } from './errors.js';
import type { Transport } from './transport.js';
import type {
  CreatePaymentParams,
  CreatePaymentResult,
  ListPaymentsParams,
  ListResult,
  MethodCatalogResponse,
  Payment,
} from './types.js';

const REPLAY_HEADER = 'PayRouters-Idempotent-Replay';

/** Drops `undefined` values and stringifies the rest. */
function toQuery(params: object): URLSearchParams {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) query.set(key, String(value));
  }
  return query;
}

function paymentPath(id: string): string {
  if (!id) throw new TypeError('payment id is required');
  return `/v1/payments/${encodeURIComponent(id)}`;
}

export class PaymentsResource {
  readonly #transport: Transport;

  constructor(transport: Transport) {
    this.#transport = transport;
  }

  /**
   * Creates a payment. POST is not retried automatically; to recover from an ambiguous
   * failure, call again with the same `order_id` and `amount`. The server returns the
   * existing pending payment (`replayed: true`) instead of creating a duplicate.
   */
  async create(params: CreatePaymentParams): Promise<CreatePaymentResult> {
    const response = await this.#transport.request<Payment>({
      method: 'POST',
      path: '/v1/payments',
      body: params,
    });
    const replayed =
      response.status === 200 || response.headers.get(REPLAY_HEADER)?.toLowerCase() === 'true';
    return { payment: response.data, replayed };
  }

  async get(id: string): Promise<Payment> {
    const response = await this.#transport.request<Payment>({ method: 'GET', path: paymentPath(id) });
    return response.data;
  }

  async list(params: ListPaymentsParams = {}): Promise<ListResult<Payment>> {
    const response = await this.#transport.request<ListResult<Payment>>({
      method: 'GET',
      path: '/v1/payments',
      query: toQuery(params),
    });
    return response.data;
  }

  /** Re-reads the provider and applies any status change. */
  async sync(id: string): Promise<Payment> {
    const response = await this.#transport.request<Payment>({
      method: 'POST',
      path: `${paymentPath(id)}/sync`,
    });
    return response.data;
  }

  /** Iterates every matching payment, following `next_cursor` page by page. */
  async *listAll(params: ListPaymentsParams = {}): AsyncGenerator<Payment, void, undefined> {
    const seen = new Set<string>();
    let cursor = params.cursor;
    for (;;) {
      const page = await this.list({ ...params, cursor });
      for (const payment of page.data) yield payment;

      const next = page.next_cursor;
      if (next === null) return;
      if (seen.has(next)) {
        throw new PayRoutersError({
          status: 200,
          code: 'invalid_response',
          message: 'PayRouters returned a pagination cursor that was already visited',
          details: { cursor: next },
        });
      }
      seen.add(next);
      cursor = next;
    }
  }
}

export class MethodsResource {
  readonly #transport: Transport;

  constructor(transport: Transport) {
    this.#transport = transport;
  }

  /** Payment methods available to this key's mode, grouped by provider. */
  async list(): Promise<MethodCatalogResponse> {
    const response = await this.#transport.request<MethodCatalogResponse>({
      method: 'GET',
      path: '/v1/methods',
    });
    return response.data;
  }
}
