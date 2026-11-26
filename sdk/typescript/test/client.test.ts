import { describe, expect, it } from 'vitest';
import {
  PayRouters,
  PayRoutersError,
  PayRoutersNetworkError,
} from '../src/index.js';

interface Recorded {
  url: string;
  method: string;
  headers: Headers;
  body: string | null;
  signal: AbortSignal | undefined;
}

type Handler = (call: number, request: Recorded) => Response | Promise<Response>;

/** Fake fetch that records every call and answers with `handler`. */
function recordingFetch(handler: Handler): { fetch: typeof fetch; calls: Recorded[] } {
  const calls: Recorded[] = [];
  const fetchImpl = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const request: Recorded = {
      url: String(input),
      method: init?.method ?? 'GET',
      headers: new Headers(init?.headers),
      body: typeof init?.body === 'string' ? init.body : null,
      signal: init?.signal ?? undefined,
    };
    calls.push(request);
    return handler(calls.length, request);
  };
  return { fetch: fetchImpl, calls };
}

function json(status: number, body: unknown, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json', ...headers },
  });
}

const API_KEY = 'prk_test_abc123';
const BASE = 'https://pay.example.com';
const PAYMENT = { id: 'pay_1', order_id: 'INV-1', status: 'pending', amount: 150000 };

function client(fetchFn: typeof fetch, extra: Partial<ConstructorParameters<typeof PayRouters>[0]> = {}): PayRouters {
  return new PayRouters({ apiKey: API_KEY, baseUrl: BASE, fetch: fetchFn, retryDelayMs: 0, ...extra });
}

const CREATE = { order_id: 'INV-1', amount: 150000 };

describe('PayRouters construction', () => {
  it('requires an API key', () => {
    expect(() => new PayRouters({ apiKey: '', baseUrl: BASE })).toThrow(TypeError);
  });

  it('sends the bearer key and JSON body on create', async () => {
    const f = recordingFetch(() => json(201, PAYMENT));
    await client(f.fetch).payments.create(CREATE);

    const [call] = f.calls;
    expect(call?.method).toBe('POST');
    expect(call?.url).toBe(`${BASE}/v1/payments`);
    expect(call?.headers.get('authorization')).toBe(`Bearer ${API_KEY}`);
    expect(call?.headers.get('content-type')).toBe('application/json');
    expect(JSON.parse(call?.body ?? 'null')).toEqual(CREATE);
  });

  it('normalises a trailing slash and path prefix on baseUrl', async () => {
    const f = recordingFetch(() => json(200, PAYMENT));
    await new PayRouters({ apiKey: API_KEY, baseUrl: 'https://host.example/payrouters/', fetch: f.fetch }).payments.get(
      'pay_1',
    );
    expect(f.calls[0]?.url).toBe('https://host.example/payrouters/v1/payments/pay_1');
  });
});

describe('payments.create replay detection', () => {
  it('reports replayed=false for a new 201 payment', async () => {
    const f = recordingFetch(() => json(201, PAYMENT));
    const result = await client(f.fetch).payments.create(CREATE);
    expect(result).toEqual({ payment: PAYMENT, replayed: false });
  });

  it('reports replayed=true when the replay header is present', async () => {
    const f = recordingFetch(() => json(200, PAYMENT, { 'PayRouters-Idempotent-Replay': 'true' }));
    const result = await client(f.fetch).payments.create(CREATE);
    expect(result.replayed).toBe(true);
    expect(result.payment).toEqual(PAYMENT);
  });

  it('treats HTTP 200 as a replay even without the header', async () => {
    const f = recordingFetch(() => json(200, PAYMENT));
    expect((await client(f.fetch).payments.create(CREATE)).replayed).toBe(true);
  });
});

describe('error envelope mapping', () => {
  it('maps the { error } envelope and x-request-id onto PayRoutersError', async () => {
    const f = recordingFetch(() =>
      json(
        409,
        { error: { code: 'order_id_conflict', message: 'order_id already used', details: { payment_id: 'pay_9' } } },
        { 'x-request-id': 'req_123' },
      ),
    );
    const failure = client(f.fetch).payments.create(CREATE);

    await expect(failure).rejects.toBeInstanceOf(PayRoutersError);
    await expect(failure).rejects.toMatchObject({
      status: 409,
      code: 'order_id_conflict',
      message: 'order_id already used',
      details: { payment_id: 'pay_9' },
      requestId: 'req_123',
    });
  });

  it('uses null details and undefined requestId when absent', async () => {
    const f = recordingFetch(() => json(422, { error: { code: 'sandbox_unsupported', message: 'no sandbox' } }));
    const error = await client(f.fetch).payments.create(CREATE).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(PayRoutersError);
    expect(error).toMatchObject({ status: 422, code: 'sandbox_unsupported', details: null });
    expect((error as PayRoutersError).requestId).toBeUndefined();
  });

  it('falls back to http_error for a non-envelope body', async () => {
    const f = recordingFetch(() => new Response('<html>bad gateway</html>', { status: 502 }));
    await expect(client(f.fetch).payments.get('pay_1')).rejects.toMatchObject({
      status: 502,
      code: 'http_error',
      details: '<html>bad gateway</html>',
    });
  });

  it('reports a 2xx body that is not JSON as invalid_response', async () => {
    const f = recordingFetch(() => new Response('ok', { status: 200 }));
    await expect(client(f.fetch).payments.get('pay_1')).rejects.toMatchObject({
      status: 200,
      code: 'invalid_response',
    });
  });
});

describe('retries', () => {
  it('retries a GET on 503 and returns the eventual success', async () => {
    const f = recordingFetch((call) => (call < 3 ? json(503, { error: { code: 'internal', message: 'busy' } }) : json(200, PAYMENT)));
    const payment = await client(f.fetch).payments.get('pay_1');
    expect(payment).toEqual(PAYMENT);
    expect(f.calls).toHaveLength(3);
  });

  it('retries a GET on 429', async () => {
    const f = recordingFetch((call) => (call === 1 ? json(429, { error: { code: 'rate_limited', message: 'slow' } }) : json(200, PAYMENT)));
    await client(f.fetch).payments.get('pay_1');
    expect(f.calls).toHaveLength(2);
  });

  it('gives up on a GET after 1 + maxRetries attempts', async () => {
    const f = recordingFetch(() => json(503, { error: { code: 'internal', message: 'down' } }));
    await expect(client(f.fetch).methods.list()).rejects.toMatchObject({ status: 503 });
    expect(f.calls).toHaveLength(3);
  });

  it('does not retry a GET on 404', async () => {
    const f = recordingFetch(() => json(404, { error: { code: 'not_found', message: 'no payment' } }));
    await expect(client(f.fetch).payments.get('pay_x')).rejects.toMatchObject({ code: 'not_found' });
    expect(f.calls).toHaveLength(1);
  });

  it('never retries a POST on 503', async () => {
    const f = recordingFetch(() => json(503, { error: { code: 'internal', message: 'down' } }));
    await expect(client(f.fetch).payments.create(CREATE)).rejects.toMatchObject({ status: 503 });
    expect(f.calls).toHaveLength(1);
  });

  it('retries a GET on network failure and then reports a transport error', async () => {
    const f = recordingFetch(() => {
      throw new TypeError('fetch failed');
    });
    const error = await client(f.fetch).payments.get('pay_1').catch((e: unknown) => e);
    expect(error).toBeInstanceOf(PayRoutersNetworkError);
    expect(error).toMatchObject({ kind: 'transport' });
    expect(f.calls).toHaveLength(3);
  });

  it('never retries a POST on network failure', async () => {
    const f = recordingFetch(() => {
      throw new TypeError('fetch failed');
    });
    await expect(client(f.fetch).payments.create(CREATE)).rejects.toBeInstanceOf(PayRoutersNetworkError);
    expect(f.calls).toHaveLength(1);
  });

  it('honours maxRetries: 0', async () => {
    const f = recordingFetch(() => json(503, { error: { code: 'internal', message: 'down' } }));
    await expect(client(f.fetch, { maxRetries: 0 }).payments.get('pay_1')).rejects.toMatchObject({ status: 503 });
    expect(f.calls).toHaveLength(1);
  });
});

describe('timeouts', () => {
  it('aborts a hung request and reports a timeout network error', async () => {
    const f = recordingFetch(
      (_call, request) =>
        new Promise<Response>((_resolve, reject) => {
          request.signal?.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')));
        }),
    );
    const error = await client(f.fetch, { timeoutMs: 10, maxRetries: 0 })
      .payments.create(CREATE)
      .catch((e: unknown) => e);
    expect(error).toBeInstanceOf(PayRoutersNetworkError);
    expect(error).toMatchObject({ kind: 'timeout' });
  });
});

describe('queries and pagination', () => {
  it('encodes list filters and skips undefined values', async () => {
    const f = recordingFetch(() => json(200, { data: [], next_cursor: null }));
    await client(f.fetch).payments.list({ order_id: 'A B', status: 'paid', limit: 10, cursor: undefined });
    const url = new URL(f.calls[0]?.url ?? '');
    expect(url.pathname).toBe('/v1/payments');
    expect(url.searchParams.get('order_id')).toBe('A B');
    expect(url.searchParams.get('status')).toBe('paid');
    expect(url.searchParams.get('limit')).toBe('10');
    expect(url.searchParams.has('cursor')).toBe(false);
  });

  it('encodes payment ids in the path', async () => {
    const f = recordingFetch(() => json(200, PAYMENT));
    await client(f.fetch).payments.sync('a/b');
    expect(f.calls[0]?.url).toBe(`${BASE}/v1/payments/a%2Fb/sync`);
    expect(f.calls[0]?.method).toBe('POST');
  });

  it('listAll follows next_cursor across pages', async () => {
    const pages: Record<string, unknown> = {
      '': { data: [{ id: 'p1' }, { id: 'p2' }], next_cursor: 'c2' },
      c2: { data: [{ id: 'p3' }], next_cursor: 'c3' },
      c3: { data: [{ id: 'p4' }], next_cursor: null },
    };
    const f = recordingFetch((_call, request) => {
      const cursor = new URL(request.url).searchParams.get('cursor') ?? '';
      return json(200, pages[cursor]);
    });

    const seen: string[] = [];
    for await (const payment of client(f.fetch).payments.listAll({ status: 'paid' })) {
      seen.push(payment.id);
    }

    expect(seen).toEqual(['p1', 'p2', 'p3', 'p4']);
    expect(f.calls.map((c) => new URL(c.url).searchParams.get('status'))).toEqual(['paid', 'paid', 'paid']);
  });

  it('listAll stops with an error if the server repeats a cursor', async () => {
    const f = recordingFetch(() => json(200, { data: [{ id: 'p1' }], next_cursor: 'loop' }));
    const seen: string[] = [];
    const attempt = (async () => {
      for await (const payment of client(f.fetch).payments.listAll()) seen.push(payment.id);
    })();
    await expect(attempt).rejects.toMatchObject({ code: 'invalid_response' });
    expect(seen).toEqual(['p1', 'p1']);
  });
});

describe('methods.list', () => {
  it('returns the provider method catalog', async () => {
    const catalog = {
      data: [
        {
          provider: 'midtrans',
          checkout_provider: true,
          methods: [{ code: 'va_bca', label: 'BCA Virtual Account', group: 'virtual_account' }],
        },
      ],
    };
    const f = recordingFetch(() => json(200, catalog));
    expect(await client(f.fetch).methods.list()).toEqual(catalog);
    expect(f.calls[0]?.url).toBe(`${BASE}/v1/methods`);
  });
});
