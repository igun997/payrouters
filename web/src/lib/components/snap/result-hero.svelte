<script lang="ts">
	/* Outcome header for terminal payments and load errors: halo icon, title, optional amount and detail. */
	import type { Snippet } from 'svelte';
	import CircleAlert from '@lucide/svelte/icons/circle-alert';
	import CircleCheck from '@lucide/svelte/icons/circle-check';
	import CircleX from '@lucide/svelte/icons/circle-x';
	import Clock from '@lucide/svelte/icons/clock';
	import Ban from '@lucide/svelte/icons/ban';

	type Tone = 'success' | 'expired' | 'failed' | 'cancelled' | 'neutral';

	let {
		tone,
		title,
		amount = null,
		detail = null,
		status,
		icon = null,
		children
	}: {
		tone: Tone;
		title: string;
		amount?: string | null;
		detail?: string | null;
		/** Value of data-status on the live region (payment status, or a page state such as `not_found`). */
		status: string;
		/** Replaces the tone's default icon, e.g. the unlink icon for a payment link that is not valid. */
		icon?: typeof CircleAlert | null;
		children?: Snippet;
	} = $props();

	// Halo is the tinted ring; the core is white with a coloured glyph, except success (solid signal).
	const tones = {
		success: { halo: 'bg-success', core: 'bg-signal text-foreground', icon: CircleCheck },
		failed: { halo: 'bg-failed', core: 'bg-card text-failed-foreground', icon: CircleX },
		expired: { halo: 'bg-expired', core: 'bg-card text-expired-foreground', icon: Clock },
		cancelled: { halo: 'bg-expired', core: 'bg-card text-expired-foreground', icon: Ban },
		neutral: { halo: 'bg-expired', core: 'bg-card text-expired-foreground', icon: CircleAlert }
	} as const;

	const t = $derived(tones[tone]);
	const Icon = $derived(icon ?? t.icon);
</script>

<section
	data-testid="snap-status"
	data-status={status}
	role="status"
	aria-live="polite"
	class="flex flex-col items-center gap-4 pt-2 text-center"
>
	<div class={`flex size-[104px] items-center justify-center rounded-full ${t.halo}`}>
		<div class={`flex size-[72px] items-center justify-center rounded-full ${t.core}`}>
			<Icon class="size-9" aria-hidden="true" />
		</div>
	</div>
	<div class="flex flex-col items-center gap-1.5">
		<h1 class="text-result-title">{title}</h1>
		{#if amount}
			<div class="amount-box w-full text-stat">
				<p
					class="tabular text-center font-bold whitespace-nowrap"
					class:amount-fit={amount.length > 12}
				>
					{amount}
				</p>
			</div>
		{/if}
		{#if detail}
			<p class="max-w-[30rem] text-body text-muted-foreground">{detail}</p>
		{/if}
	</div>
	{#if children}{@render children()}{/if}
</section>
