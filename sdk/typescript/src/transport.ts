import { PayRoutersError, PayRoutersNetworkError } from './errors.js';

export interface TransportOptions {
  apiKey: string;
  /** Origin (and optional path prefix) with no trailing slash. */
  baseUrl: string;
  timeoutMs: number;
  fetch: typeof fetch | undefined;
  /** Extra attempts for GET after a network error, 5xx or 429. POST is never retried. */
  maxRetries: number;
  /** Delay before the first retry; doubles on each subsequent retry. */
  retryDelayMs: number;
}

export interface RequestSpec {
  method: 'GET' | 'POST';
  /** Path beginning with `/v1/…`. */
  path: string;
  query?: URLSearchParams | undefined;
  body?: unknown;
}

export interface ApiResponse<T> {
  data: T;
  status: number;
  headers: Headers;
}

interface RawResponse {
  status: number;
  headers: Headers;
  text: string;
}

interface ErrorEnvelope {
  code: string;
  message: string;
  details: unknown;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

/** Parses `{ "error": { "code", "message", "details" } }`; `undefined` for any other body. */
function parseErrorEnvelope(text: string): ErrorEnvelope | undefined {
  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch {
    return undefined;
  }
  if (!isRecord(json) || !isRecord(json.error)) return undefined;
  const { code, message, details } = json.error;
  if (typeof code !== 'string' || typeof message !== 'string') return undefined;
  return { code, message, details: details ?? null };
}

function apiError(raw: RawResponse, requestId: string | undefined): PayRoutersError {
  const envelope = parseErrorEnvelope(raw.text);
  if (envelope) {
    return new PayRoutersError({
      status: raw.status,
      code: envelope.code,
      message: envelope.message,
      details: envelope.details,
      requestId,
    });
  }
  return new PayRoutersError({
    status: raw.status,
    code: 'http_error',
    message: `PayRouters request failed with HTTP ${raw.status}`,
    details: raw.text.slice(0, 500) || null,
    requestId,
  });
}

export class Transport {
  readonly #options: TransportOptions;

  constructor(options: TransportOptions) {
    this.#options = options;
  }

  async request<T>(spec: RequestSpec): Promise<ApiResponse<T>> {
    const retryable = spec.method === 'GET';
    const maxAttempts = retryable ? this.#options.maxRetries + 1 : 1;

    for (let attempt = 1; ; attempt++) {
      const isLast = attempt >= maxAttempts;
      let raw: RawResponse;
      try {
        raw = await this.#send(spec);
      } catch (error) {
        if (isLast || !(error instanceof PayRoutersNetworkError)) throw error;
        await this.#backoff(attempt);
        continue;
      }
      if (retryable && !isLast && (raw.status === 429 || raw.status >= 500)) {
        await this.#backoff(attempt);
        continue;
      }
      return this.#decode<T>(raw);
    }
  }

  async #backoff(attempt: number): Promise<void> {
    const delay = this.#options.retryDelayMs * 2 ** (attempt - 1);
    // Executor form on purpose: Promise.withResolvers needs Node 22, and this package supports Node 18.
    await new Promise<void>((resolve) => setTimeout(resolve, delay));
  }

  /** One HTTP attempt, including reading the body, under the request timeout. */
  async #send(spec: RequestSpec): Promise<RawResponse> {
    const { apiKey, baseUrl, timeoutMs } = this.#options;
    const query = spec.query?.toString();
    const url = query ? `${baseUrl}${spec.path}?${query}` : `${baseUrl}${spec.path}`;

    const headers: Record<string, string> = {
      Authorization: `Bearer ${apiKey}`,
      Accept: 'application/json',
    };
    let body: string | undefined;
    if (spec.body !== undefined) {
      headers['Content-Type'] = 'application/json';
      body = JSON.stringify(spec.body);
    }

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const response = await this.#fetch(url, {
        method: spec.method,
        headers,
        body: body ?? null,
        signal: controller.signal,
      });
      const text = await response.text();
      return { status: response.status, headers: response.headers, text };
    } catch (cause) {
      if (controller.signal.aborted) {
        throw new PayRoutersNetworkError({
          kind: 'timeout',
          message: `PayRouters request timed out after ${timeoutMs} ms`,
          cause,
        });
      }
      const reason = cause instanceof Error ? cause.message : String(cause);
      throw new PayRoutersNetworkError({
        kind: 'transport',
        message: `PayRouters request failed: ${reason}`,
        cause,
      });
    } finally {
      clearTimeout(timer);
    }
  }

  #fetch(url: string, init: RequestInit): Promise<Response> {
    // Call through the global directly: a method call on the options object would
    // rebind `this` and break runtimes that require the global receiver.
    return this.#options.fetch ? this.#options.fetch(url, init) : globalThis.fetch(url, init);
  }

  #decode<T>(raw: RawResponse): ApiResponse<T> {
    const requestId = raw.headers.get('x-request-id') ?? undefined;
    if (raw.status < 200 || raw.status >= 300) {
      throw apiError(raw, requestId);
    }
    let data: T;
    try {
      data = JSON.parse(raw.text) as T;
    } catch {
      throw new PayRoutersError({
        status: raw.status,
        code: 'invalid_response',
        message: 'PayRouters returned a response body that is not valid JSON',
        details: raw.text.slice(0, 500) || null,
        requestId,
      });
    }
    return { data, status: raw.status, headers: raw.headers };
  }
}
