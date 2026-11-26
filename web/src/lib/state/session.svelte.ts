/**
 * Session state for the admin dashboard. An API call answered 401 with a stored token marks the
 * session expired. `inShell` is true while the admin layout is mounted: the layout then shows the
 * session-ended dialog over the current page instead of redirecting to the login page.
 */
class Session {
	expired = $state(false);
	inShell = $state(false);

	markExpired(): void {
		this.expired = true;
	}

	reset(): void {
		this.expired = false;
	}
}

export const session = new Session();
