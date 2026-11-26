import { redirect } from '@sveltejs/kit';
import { getToken } from '#lib/api/client';
import type { LayoutLoad } from './$types';

// Client-side guard: every admin page requires a stored admin token.
export const load: LayoutLoad = ({ url }) => {
	if (!getToken()) {
		redirect(307, `/login?next=${encodeURIComponent(url.pathname + url.search)}`);
	}
	return {};
};
