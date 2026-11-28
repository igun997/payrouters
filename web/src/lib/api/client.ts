import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import type { ApiErrorEnvelope } from '#lib/api/types';
import { session } from '#lib/state/session.svelte';

export const TOKEN_KEY = 'payrouters.admin_token';

export function getToken(): string | null {
	if (typeof localStorage === 'undefined') return null;
	return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string): void {
	localStorage.setItem(TOKEN_KEY, token.trim());
}

export function clearToken(): void {
	localStorage.removeItem(TOKEN_KEY);
}

/** An error returned by the server in the `{ error: { code, message, details } }` envelope. */
export class ApiError extends Error {
	readonly status: number;
	readonly code: string;
	readonly details: unknown;
	/** Seconds until a rate-limited request may be retried (from `Retry-After`); null otherwise. */
	readonly retryAfter: number | null;

	constructor(
		status: number,
		code: string,
		message: string,
		details: unknown = null,
		retryAfter: number | null = null
	) {
		super(message);
		this.name = 'ApiError';
		this.status = status;
		this.code = code;
		this.details = details;
		this.retryAfter = retryAfter;
	}
}

export type QueryValue = string | number | boolean | null | undefined;
export type Query = Record<string, QueryValue>;

export interface RequestOptions {
	method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
	query?: Query;
	body?: unknown;
	/** Bearer token to send. Defaults to the stored admin token; `null` sends no Authorization header. */
	token?: string | null;
	/** On 401 with a token: clear it and go to /login. Default true. */
	redirectOn401?: boolean;
	signal?: AbortSignal;
}

export function buildUrl(path: string, query?: Query): string {
	const params = new URLSearchParams();
	for (const [key, value] of Object.entries(query ?? {})) {
		if (value === undefined || value === null || value === '') continue;
		params.set(key, String(value));
	}
	const qs = params.toString();
	return qs ? `${path}?${qs}` : path;
}

function isErrorEnvelope(value: unknown): value is ApiErrorEnvelope {
	if (typeof value !== 'object' || value === null || !('error' in value)) return false;
	const error = value.error;
	return (
		typeof error === 'object' &&
		error !== null &&
		'code' in error &&
		typeof error.code === 'string' &&
		'message' in error &&
		typeof error.message === 'string'
	);
}

async function toApiError(res: Response): Promise<ApiError> {
	let code = 'http_error';
	let message = res.statusText || `Request failed (${res.status})`;
	let details: unknown = null;
	// Only the seconds form is sent by the server; an HTTP-date is treated as unknown.
	const retryAfter = /^\d+$/.test(res.headers.get('Retry-After') ?? '')
		? Number(res.headers.get('Retry-After'))
		: null;
	try {
		const body: unknown = await res.json();
		if (isErrorEnvelope(body)) {
			code = body.error.code;
			message = body.error.message;
			details = body.error.details ?? null;
		}
	} catch {
		// Non-JSON error body (e.g. a proxy page): keep the status-based message.
	}
	return new ApiError(res.status, code, message, details, retryAfter);
}

function redirectToLogin(): void {
	if (window.location.pathname.startsWith('/login')) return;
	const next = window.location.pathname + window.location.search;
	// eslint-disable-next-line svelte/no-navigation-without-resolve -- resolve() supplies the path; only the next query is appended
	void goto(`${resolve('/login')}?next=${encodeURIComponent(next)}`, { replaceState: true });
}

/** Performs the request and returns the raw Response; throws ApiError on non-2xx. */
export async function rawRequest(path: string, options: RequestOptions = {}): Promise<Response> {
	const token = options.token === undefined ? getToken() : options.token;
	const headers = new Headers({ Accept: 'application/json' });
	if (options.body !== undefined) headers.set('Content-Type', 'application/json');
	if (token) headers.set('Authorization', `Bearer ${token}`);

	let res: Response;
	try {
		res = await fetch(buildUrl(path, options.query), {
			method: options.method ?? 'GET',
			headers,
			body: options.body === undefined ? undefined : JSON.stringify(options.body),
			signal: options.signal
		});
	} catch (cause) {
		if (cause instanceof DOMException && cause.name === 'AbortError') throw cause;
		throw new ApiError(0, 'network_error', 'Cannot reach the server. Check your connection.', null);
	}

	if (res.status === 401 && token && (options.redirectOn401 ?? true)) {
		clearToken();
		session.markExpired();
		// Inside the admin shell the session dialog offers sign-in; elsewhere keep the login redirect.
		if (!session.inShell) redirectToLogin();
	}
	if (!res.ok) throw await toApiError(res);
	return res;
}

/** Performs the request and parses the JSON body. `T` is the contract type for the endpoint. */
export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
	const res = await rawRequest(path, options);
	// The server's response shape is the contract; the caller picks the matching type.
	return (await res.json()) as T;
}

/** For endpoints that answer 204 No Content (deletes). */
export async function requestVoid(path: string, options: RequestOptions = {}): Promise<void> {
	await rawRequest(path, options);
}

/** Downloads an authenticated file (e.g. CSV export) by fetching it with the bearer header. */
export async function downloadFile(path: string, query: Query, filename: string): Promise<void> {
	const res = await rawRequest(path, { query });
	const url = URL.createObjectURL(await res.blob());
	const anchor = document.createElement('a');
	anchor.href = url;
	anchor.download = filename;
	document.body.append(anchor);
	anchor.click();
	anchor.remove();
	setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function errorMessage(error: unknown): string {
	if (error instanceof ApiError) return error.message;
	if (error instanceof Error) return error.message;
	return 'Something went wrong. Please try again.';
}
