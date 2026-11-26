import type {
	CredentialField,
	MetaProvider,
	Provider,
	ProviderAccount,
	Mode
} from '#lib/api/types';
import { feesToRows, type FeeRow } from '#lib/fees';
import { DEFAULT_CHAIN_ID } from '#lib/provider-display';

export type ProviderFormValues = {
	provider: Provider | '';
	mode: Mode;
	label: string;
	enabled: boolean;
	/** Keyed by credential key. Secret keys start empty: a blank secret keeps the saved one. */
	credentials: Record<string, string>;
	fees: FeeRow[];
};

export type ProviderFormErrors = { provider?: string; credentials: Record<string, string> };

export function blankProviderForm(): ProviderFormValues {
	return { provider: '', mode: 'sandbox', label: '', enabled: true, credentials: {}, fees: [] };
}

/** Default credential values: non-secret fields start empty, the chain starts on Base. */
export function defaultCredentials(fields: CredentialField[]): Record<string, string> {
	return Object.fromEntries(
		fields.map((field) => [field.key, field.key === 'default_chain_id' ? DEFAULT_CHAIN_ID : ''])
	);
}

/** Edit form values. Non-secret credentials are prefilled; secrets stay blank (they are masked). */
export function valuesFromAccount(
	account: ProviderAccount,
	provider: MetaProvider | undefined
): ProviderFormValues {
	const credentials = Object.fromEntries(
		(provider?.credential_fields ?? [])
			.filter((field) => !field.secret)
			.map((field) => [field.key, account.credentials[field.key] ?? ''])
	);
	return {
		provider: account.provider,
		mode: account.mode,
		label: account.label,
		enabled: account.enabled,
		credentials,
		fees: feesToRows(account.fees)
	};
}

/** Credentials to send: only fields the form defines, trimmed, blanks dropped (blank keeps the saved value). */
export function credentialPayload(
	values: ProviderFormValues,
	fields: CredentialField[]
): Record<string, string> {
	return Object.fromEntries(
		fields
			.map((field) => [field.key, (values.credentials[field.key] ?? '').trim()] as const)
			.filter(([, value]) => value !== '')
	);
}

export function validateProviderForm(
	values: ProviderFormValues,
	mode: 'create' | 'edit',
	provider: MetaProvider | undefined
): ProviderFormErrors {
	const errors: ProviderFormErrors = { credentials: {} };
	if (mode === 'create') {
		if (!values.provider) errors.provider = 'Choose a provider';
		for (const field of provider?.credential_fields ?? []) {
			if (field.required && !(values.credentials[field.key] ?? '').trim()) {
				errors.credentials[field.key] = `Enter ${field.label}`;
			}
		}
	}
	return errors;
}

export const NO_PROVIDER_ERRORS: ProviderFormErrors = { credentials: {} };
