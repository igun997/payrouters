import type { Mode } from '#lib/api/types';

const MODE_KEY = 'payrouters.mode';

function readMode(): Mode {
	if (typeof localStorage === 'undefined') return 'sandbox';
	return localStorage.getItem(MODE_KEY) === 'live' ? 'live' : 'sandbox';
}

/** Dashboard-wide preferences, persisted in localStorage. Sandbox is the default. */
class Prefs {
	mode = $state<Mode>(readMode());

	setMode(mode: Mode): void {
		this.mode = mode;
		if (typeof localStorage !== 'undefined') localStorage.setItem(MODE_KEY, mode);
	}
}

export const prefs = new Prefs();
