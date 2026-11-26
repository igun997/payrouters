import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

// Unknown admin URLs render the in-shell Not Found page (see (admin)/+error.svelte).
export const load: PageLoad = () => {
	error(404, 'Page not found');
};
