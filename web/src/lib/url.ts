/** Returns the URL if it is an absolute http(s) URL, otherwise null. Guards redirects and links. */
export function safeHttpUrl(value: string | null | undefined): string | null {
	if (!value) return null;
	try {
		const url = new URL(value);
		return url.protocol === 'https:' || url.protocol === 'http:' ? url.toString() : null;
	} catch {
		return null;
	}
}

/** Opens a provider or merchant URL in a new tab, if it is a valid http(s) URL. */
export function openExternal(value: string | null | undefined): void {
	const url = safeHttpUrl(value);
	if (url) window.open(url, '_blank', 'noopener,noreferrer');
}

/** Navigates the current tab to a provider or merchant URL, if it is a valid http(s) URL. */
export function navigateTo(value: string | null | undefined): void {
	const url = safeHttpUrl(value);
	if (url) window.location.assign(url);
}
