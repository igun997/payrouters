// Labels for payment methods and bank codes on the Snap page. Codes follow docs/api.md.
import type { Method } from '#lib/api/types';

export const METHOD_LABEL: Record<Method, string> = {
	va_bca: 'BCA Virtual Account',
	va_bni: 'BNI Virtual Account',
	va_bri: 'BRI Virtual Account',
	va_mandiri: 'Mandiri Virtual Account',
	va_permata: 'Permata Virtual Account',
	qris: 'QRIS'
};

/** Short badge shown in method rows and account cards. Mandiri is shown as "MDR" and Permata as "PMT". */
const BADGE: Record<string, string> = {
	va_bca: 'BCA',
	va_bni: 'BNI',
	va_bri: 'BRI',
	va_mandiri: 'MDR',
	va_permata: 'PMT',
	qris: 'QRIS'
};

/** Badge for a bank code from the `instructions.bank` field (lower case, e.g. "bca"). */
export function bankBadge(bank: string): string {
	const key = `va_${bank.toLowerCase()}`;
	return BADGE[key] ?? bank.slice(0, 3).toUpperCase();
}

/** Codes the server has a logo for (`GET /assets/logo/<code>`). */
const LOGO_CODES = ['bca', 'bni', 'bri', 'mandiri', 'permata', 'qris'] as const;

/** The logo code for a bank code, a `va_*` method code or `qris`; null when there is no logo. */
export function logoCode(code: string): string | null {
	const key = code.toLowerCase().replace(/^va_/, '');
	return (LOGO_CODES as readonly string[]).includes(key) ? key : null;
}

/** Full name for a bank code from the `instructions.bank` field. */
export function bankName(bank: string): string {
	const key = `va_${bank.toLowerCase()}` as Method;
	return METHOD_LABEL[key] ?? `${bank.toUpperCase()} Virtual Account`;
}

/** Short name used in progress copy, e.g. "Connecting to BNI". */
export function methodShortName(code: string): string {
	return BADGE[code] ?? code;
}
