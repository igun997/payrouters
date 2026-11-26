<script lang="ts">
	import Pill from './pill.svelte';
	import type { WebhookOutcome } from '#lib/api/types';

	/**
	 * Inbox outcome of one provider webhook. applied = success, noop = neutral, unmatched = pending,
	 * rejected and error = failed. Unknown values fall back to a neutral pill with the raw text.
	 */
	let { outcome }: { outcome: WebhookOutcome } = $props();

	type Tone = 'success' | 'neutral' | 'pending' | 'failed';
	const outcomes: Record<WebhookOutcome, { label: string; tone: Tone }> = {
		applied: { label: 'Applied', tone: 'success' },
		noop: { label: 'No-op', tone: 'neutral' },
		unmatched: { label: 'Unmatched', tone: 'pending' },
		rejected: { label: 'Rejected', tone: 'failed' },
		error: { label: 'Error', tone: 'failed' }
	};
	const known: Partial<Record<string, { label: string; tone: Tone }>> = outcomes;
	const fallback = (label: string): { label: string; tone: Tone } => ({ label, tone: 'neutral' });
	const shown = $derived(known[outcome] ?? fallback(outcome));
</script>

<span data-testid="outcome-pill" data-outcome={outcome}>
	<Pill tone={shown.tone}>{shown.label}</Pill>
</span>
