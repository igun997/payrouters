import { toast } from 'svelte-sonner';

/**
 * Transient feedback. Success, info and plain errors dismiss after 4 s. An error with an action
 * stays until it is dismissed, so the action can still be taken. Persistent problems belong in a
 * `Banner`, not a toast.
 */
const DURATION_MS = 4000;

export const notify = {
	success: (message: string) => toast.success(message, { duration: DURATION_MS }),
	info: (message: string) => toast.info(message, { duration: DURATION_MS }),
	error: (message: string) => toast.error(message, { duration: DURATION_MS }),
	errorWithAction: (message: string, action: { label: string; onClick: () => void }) =>
		toast.error(message, { duration: Number.POSITIVE_INFINITY, action })
};
