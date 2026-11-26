import { adminApi } from '#lib/api/admin';
import type { Meta } from '#lib/api/types';

let cached: Promise<Meta> | null = null;

/** The server's catalog (providers, methods, credential fields). Fetched once per session. */
export function loadMeta(): Promise<Meta> {
	cached ??= adminApi.meta().catch((error: unknown) => {
		cached = null;
		throw error;
	});
	return cached;
}

/** Method code → human label, from the server catalog. Unknown codes fall back to the code. */
export function methodLabels(meta: Meta | undefined): Record<string, string> {
	const labels: Record<string, string> = {};
	for (const method of meta?.methods ?? []) labels[method.code] = method.label;
	return labels;
}

/** Provider id → human label, from the server catalog. */
export function providerLabels(meta: Meta | undefined): Record<string, string> {
	const labels: Record<string, string> = {};
	for (const provider of meta?.providers ?? []) labels[provider.id] = provider.label;
	return labels;
}
