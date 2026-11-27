// OAuth (MCP) helpers for the consent and integrations specs: dynamic client registration, PKCE,
// the authorization URL, the browser round trip to the callback, the token exchange and JSON-RPC
// calls to /mcp. Everything goes through the same endpoints a real MCP client uses.
import { createHash, randomBytes } from 'node:crypto';
import type { APIRequestContext, Page } from '@playwright/test';
import { uid } from './api';

/** Registered by every test client. Port 9 is answered by the browser route, never by a server. */
export const REDIRECT_URI = 'http://127.0.0.1:9/callback';

export interface OAuthClient {
	client_id: string;
	client_name: string;
	redirect_uris: string[];
}

export interface Authorization {
	client: OAuthClient;
	verifier: string;
	state: string;
	/** `/oauth/authorize?...` with every parameter a real client sends. */
	path: string;
}

export interface TokenResponse {
	access_token: string;
	token_type: string;
	expires_in: number;
	refresh_token: string;
	scope: string;
}

export interface RpcResult {
	tools?: { name: string }[];
	isError?: boolean;
	content?: { type: string; text: string }[];
	[key: string]: unknown;
}

export interface RpcResponse {
	jsonrpc: string;
	id: number | null;
	result?: RpcResult;
	error?: { code: number; message: string };
}

/** RFC 7636 S256 pair: a random verifier and its SHA-256 challenge. */
export function pkce(): { verifier: string; challenge: string } {
	const verifier = randomBytes(32).toString('base64url');
	const challenge = createHash('sha256').update(verifier).digest('base64url');
	return { verifier, challenge };
}

export async function registerClient(
	req: APIRequestContext,
	clientName = uid('Assistant')
): Promise<OAuthClient> {
	const res = await req.post('/oauth/register', {
		data: {
			client_name: clientName,
			redirect_uris: [REDIRECT_URI],
			token_endpoint_auth_method: 'none'
		}
	});
	if (res.status() !== 201) throw new Error(`register -> ${res.status()} ${await res.text()}`);
	return (await res.json()) as OAuthClient;
}

/**
 * Registers a client and builds its authorization request. `redirectUri` and `scope` override the
 * defaults (the registered callback and an empty scope, which means `read write`).
 */
export async function newAuthorization(
	req: APIRequestContext,
	baseURL: string | undefined,
	options: { redirectUri?: string; scope?: string; client?: OAuthClient } = {}
): Promise<Authorization> {
	const client = options.client ?? (await registerClient(req));
	const { verifier, challenge } = pkce();
	const state = uid('state');
	const params = new URLSearchParams({
		response_type: 'code',
		client_id: client.client_id,
		redirect_uri: options.redirectUri ?? REDIRECT_URI,
		code_challenge: challenge,
		code_challenge_method: 'S256',
		state,
		resource: new URL('/mcp', baseURL).toString()
	});
	if (options.scope !== undefined) params.set('scope', options.scope);
	return { client, verifier, state, path: `/oauth/authorize?${params.toString()}` };
}

/** Answers the browser's navigation to the callback itself, so the code stays in the address bar. */
export async function interceptCallback(page: Page): Promise<void> {
	await page.route(/^http:\/\/127\.0\.0\.1:9\/callback/, (route) =>
		route.fulfill({
			status: 200,
			contentType: 'text/html',
			body: '<!doctype html><title>Callback</title><p>Callback received</p>'
		})
	);
}

/** Resolves with the callback address the browser navigated to (code or error, and state). */
export async function waitForCallback(page: Page): Promise<URL> {
	await page.waitForURL(
		(url) => url.origin === 'http://127.0.0.1:9' && url.pathname === '/callback'
	);
	return new URL(page.url());
}

/** Exchanges the authorization code at /oauth/token (form encoded, PKCE verifier included). */
export async function exchangeCode(
	req: APIRequestContext,
	auth: Authorization,
	code: string
): Promise<{ status: number; json: TokenResponse }> {
	const res = await req.post('/oauth/token', {
		form: {
			grant_type: 'authorization_code',
			code,
			redirect_uri: REDIRECT_URI,
			client_id: auth.client.client_id,
			code_verifier: auth.verifier
		}
	});
	return { status: res.status(), json: (await res.json()) as TokenResponse };
}

/** One JSON-RPC request to /mcp. Without a token the server answers 401 with a challenge. */
export async function mcp(
	req: APIRequestContext,
	token: string | null,
	method: string,
	params?: unknown
): Promise<{ status: number; body: RpcResponse | null }> {
	const res = await req.post('/mcp', {
		headers: {
			Accept: 'application/json, text/event-stream',
			...(token ? { Authorization: `Bearer ${token}` } : {})
		},
		data: { jsonrpc: '2.0', id: 1, method, params }
	});
	const body = res.status() === 200 ? ((await res.json()) as RpcResponse) : null;
	return { status: res.status(), body };
}

export const INITIALIZE_PARAMS = {
	protocolVersion: '2025-11-25',
	capabilities: {},
	clientInfo: { name: 'payrouters-e2e', version: '1.0.0' }
};

export function callTool(
	req: APIRequestContext,
	token: string,
	name: string,
	args: Record<string, unknown>
): Promise<{ status: number; body: RpcResponse | null }> {
	return mcp(req, token, 'tools/call', { name, arguments: args });
}
