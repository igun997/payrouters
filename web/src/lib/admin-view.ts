// Display helpers for the admin dashboard. English copy and Indonesian numbers, Asia/Jakarta times.
import { TZ } from '#lib/format';

const PROVIDER_MARK: Record<string, string> = {
	midtrans: 'MT',
	xendit: 'XD',
	idrx: 'ID',
	mock: 'MK'
};

/** Two-letter tile text for a provider (MT, XD, ID). */
export function providerMark(provider: string): string {
	return PROVIDER_MARK[provider] ?? provider.slice(0, 2).toUpperCase();
}

const dayMonth = new Intl.DateTimeFormat('en-GB', {
	day: 'numeric',
	month: 'short',
	timeZone: 'UTC'
});
const dayMonthYear = new Intl.DateTimeFormat('en-GB', {
	day: 'numeric',
	month: 'short',
	year: 'numeric',
	timeZone: 'UTC'
});
const clock = new Intl.DateTimeFormat('id-ID', {
	hour: '2-digit',
	minute: '2-digit',
	hour12: false,
	timeZone: TZ
});
const compactIdr = new Intl.NumberFormat('id-ID', {
	notation: 'compact',
	maximumFractionDigits: 1
});
const percent = new Intl.NumberFormat('id-ID', { style: 'percent', maximumFractionDigits: 1 });
const percentDecimal = new Intl.NumberFormat('id-ID', {
	style: 'percent',
	maximumFractionDigits: 1,
	minimumFractionDigits: 1
});

/** "26 Nov" for a YYYY-MM-DD day. */
export function shortDay(ymd: string): string {
	const date = new Date(`${ymd}T00:00:00Z`);
	return Number.isNaN(date.getTime()) ? ymd : dayMonth.format(date);
}

/** "28 Oct – 26 Nov" */
export function rangeLabel(from: string, to: string): string {
	return `${shortDay(from)} – ${shortDay(to)}`;
}

/** Full label with year, e.g. "26 Nov 2026". */
export function fullDay(ymd: string): string {
	const date = new Date(`${ymd}T00:00:00Z`);
	return Number.isNaN(date.getTime()) ? ymd : dayMonthYear.format(date);
}

/** Number of days in an inclusive YYYY-MM-DD range. */
export function inclusiveDays(from: string, to: string): number {
	const a = Date.parse(`${from}T00:00:00Z`);
	const b = Date.parse(`${to}T00:00:00Z`);
	if (Number.isNaN(a) || Number.isNaN(b)) return 0;
	return Math.max(1, Math.round((b - a) / 86_400_000) + 1);
}

/** "19.47" in Asia/Jakarta. */
export function clockJakarta(iso: string | null | undefined): string {
	if (!iso) return '—';
	const date = new Date(iso);
	return Number.isNaN(date.getTime()) ? iso : clock.format(date);
}

/** "26 Nov, 19.47" for lists. */
export function listDateTime(iso: string | null | undefined): string {
	if (!iso) return '—';
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return iso;
	return `${dayMonth.format(new Date(date.toLocaleString('en-US', { timeZone: TZ })))}, ${clock.format(date)}`;
}

/** "26 Nov 2026, 19.40 WIB" for headings. */
export function fullDateTime(iso: string | null | undefined): string {
	if (!iso) return '—';
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return iso;
	return `${dayMonthYear.format(new Date(date.toLocaleString('en-US', { timeZone: TZ })))}, ${clock.format(date)} WIB`;
}

/** Compact rupiah for chart axes, e.g. "2,4 jt". */
export function compactRupiah(value: number): string {
	return compactIdr.format(value);
}

/** Ratio to a percent with a decimal comma, e.g. 0.834 -> "83,4%". */
export function formatRatio(ratio: number | null | undefined): string {
	return ratio == null ? '—' : percent.format(ratio);
}

/** Percent with one fixed decimal, e.g. 0.007 -> "0,7%". */
export function formatRatioFixed(ratio: number | null | undefined): string {
	return ratio == null ? '—' : percentDecimal.format(ratio);
}

/** Share of a total as a ratio (0 when the total is zero). */
export function shareOf(part: number, total: number): number {
	return total > 0 ? part / total : 0;
}

/** Splits a VA or bill number into groups of four: "8588 0499 5978 4". */
export function groupDigits(value: string): string {
	return value.replace(/\s+/g, '').replace(/(.{4})(?!$)/g, '$1 ');
}
