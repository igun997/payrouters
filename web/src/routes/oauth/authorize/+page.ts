import { redirect } from '@sveltejs/kit';
import { getToken } from '#lib/api/client';
import type { PageLoad } from './$types';

// The consent screen needs the dashboard session. Without a stored admin token, sign in first and
// come back here with the same query string, so the authorization request is not lost.
export const load: PageLoad = ({ url }) => {
	if (!getToken()) {
		redirect(307, `/login?next=${encodeURIComponent(url.pathname + url.search)}`);
	}
	return {};
};
