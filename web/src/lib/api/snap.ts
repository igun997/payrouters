import { request } from '#lib/api/client';
import type { PayChoice, SimulateOutcome, SnapState } from '#lib/api/types';

// Public Snap API: the token in the URL is the capability, so no Authorization header is sent.
const base = (token: string) => `/snap/api/${encodeURIComponent(token)}`;

export const snapApi = {
	get: (token: string) => request<SnapState>(base(token), { token: null }),
	/** Exactly one of `{ method }` (a standard method) or `{ provider }` (that provider's checkout). */
	pay: (token: string, choice: PayChoice) =>
		request<SnapState>(`${base(token)}/pay`, { method: 'POST', body: choice, token: null }),
	/** Starts an expired payment over: the same payment, reopened for its original window. */
	restart: (token: string) =>
		request<SnapState>(`${base(token)}/restart`, { method: 'POST', token: null }),
	simulate: (token: string, outcome: SimulateOutcome) =>
		request<SnapState>(`${base(token)}/simulate`, {
			method: 'POST',
			body: { outcome },
			token: null
		})
};
