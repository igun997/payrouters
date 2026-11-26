import { TZ } from '#lib/format';

const day = new Intl.DateTimeFormat('id-ID', {
	timeZone: TZ,
	day: '2-digit',
	month: 'short',
	year: 'numeric'
});
const dayTime = new Intl.DateTimeFormat('id-ID', {
	timeZone: TZ,
	day: '2-digit',
	month: 'short',
	hour: '2-digit',
	minute: '2-digit',
	hour12: false
});

/** "12 Nov 2026" in Asia/Jakarta. */
export function keyDate(iso: string): string {
	const date = new Date(iso);
	return Number.isNaN(date.getTime()) ? iso : day.format(date);
}

/** "26 Nov, 19.47 WIB" in Asia/Jakarta, or an em dash when the key was never used. */
export function keyLastUsed(iso: string | null): string {
	if (!iso) return '—';
	const date = new Date(iso);
	return Number.isNaN(date.getTime()) ? iso : `${dayTime.format(date).replace(':', '.')} WIB`;
}
