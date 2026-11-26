<script lang="ts">
	/* Time left to pay. Ticks every second; `aria-live` is off so screen readers do not announce each tick. */
	import Clock from '@lucide/svelte/icons/clock';
	import { formatCountdown } from '#lib/format';
	import { cn } from '#lib/utils';

	let { expiresAt, label }: { expiresAt: string; label: string } = $props();

	let now = $state(Date.now());

	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	const value = $derived(formatCountdown(expiresAt, now));
	const expired = $derived(value === 'expired');
</script>

<div
	class={cn(
		'flex items-center justify-between gap-3 rounded-md px-3.5 py-2.5',
		expired ? 'bg-expired text-expired-foreground' : 'bg-pending text-pending-foreground'
	)}
>
	<span class="flex min-w-0 items-center gap-2 text-label">
		<Clock class="size-4 shrink-0" aria-hidden="true" />
		{label}
	</span>
	<span
		data-testid="snap-countdown"
		aria-live="off"
		class="tabular font-mono text-label font-semibold whitespace-nowrap"
	>
		{expired ? 'Expired' : value}
	</span>
</div>
