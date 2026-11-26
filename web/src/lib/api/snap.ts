import { request } from '#lib/api/client';
import type { SimulateOutcome, SnapState } from '#lib/api/types';

// Public Snap API: the token in the URL is the capability, so no Authorization header is sent.
const base = (token: string) => `/snap/api/${encodeURIComponent(token)}`;

export const snapApi = {
	get: (token: string) => request<SnapState>(base(token), { token: null }),
	pay: (token: string, method: string) =>
		request<SnapState>(`${base(token)}/pay`, { method: 'POST', body: { method }, token: null }),
	simulate: (token: string, outcome: SimulateOutcome) =>
		request<SnapState>(`${base(token)}/simulate`, {
			method: 'POST',
			body: { outcome },
			token: null
		})
};
