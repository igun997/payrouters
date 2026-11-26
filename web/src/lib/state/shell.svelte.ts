import type { Snippet } from 'svelte';
import { presetRange, type RangePreset } from '#lib/format';

export type DateRange = { preset: RangePreset; from: string; to: string };

/**
 * Admin top bar state. Pages call `shell.set` on mount and `shell.clear` on destroy; the layout
 * falls back to a route title for pages that do not set one. The date range is shared by the
 * pages that show the top-bar range control (Overview, Cash-ins) and by Payments. `actions` are the
 * page's primary buttons; they render in the top bar on desktop and in the content on phones.
 */
class Shell {
	title = $state('');
	/** Shorter top-bar title for phones; the document title keeps the full one. */
	phoneTitle = $state<string | null>(null);
	subtitle = $state<string | null>(null);
	/** Show the date range control in the top bar (desktop). */
	rangeControl = $state(false);
	actions = $state.raw<Snippet | null>(null);
	range = $state<DateRange>({ preset: '30d', ...presetRange('30d') });

	set(next: {
		title: string;
		phoneTitle?: string | null;
		subtitle?: string | null;
		rangeControl?: boolean;
		actions?: Snippet | null;
	}): void {
		this.title = next.title;
		this.phoneTitle = next.phoneTitle ?? null;
		this.subtitle = next.subtitle ?? null;
		this.rangeControl = next.rangeControl ?? false;
		this.actions = next.actions ?? null;
	}

	clear(): void {
		this.title = '';
		this.phoneTitle = null;
		this.subtitle = null;
		this.rangeControl = false;
		this.actions = null;
	}

	setRange(range: DateRange): void {
		this.range = range;
	}
}

export const shell = new Shell();
