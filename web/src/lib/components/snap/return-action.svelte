<script lang="ts">
	/*
	 * Sends the customer back to the merchant after a terminal status. Counts down, can be cancelled;
	 * the button is always available.
	 */
	import { Button } from '#lib/components/ui/button';
	import { navigateTo } from '#lib/url';

	const SECONDS = 3;

	let { url, label }: { url: string; label: string } = $props();

	let left = $state(SECONDS);
	let cancelled = $state(false);
	const host = $derived(new URL(url).host);

	$effect(() => {
		if (cancelled) return;
		if (left <= 0) {
			navigateTo(url);
			return;
		}
		const timer = setTimeout(() => (left -= 1), 1000);
		return () => clearTimeout(timer);
	});
</script>

<div class="flex flex-col items-center gap-3 text-center">
	{#if !cancelled}
		<p
			class="flex flex-wrap items-center justify-center gap-x-2 text-helper text-muted-foreground"
			role="status"
		>
			<span>Returning to {host} in {left} {left === 1 ? 'second' : 'seconds'}…</span>
			<button
				type="button"
				class="font-semibold text-foreground underline underline-offset-4 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
				onclick={() => (cancelled = true)}
			>
				Cancel
			</button>
		</p>
	{/if}
	<Button size="lg" class="w-full" onclick={() => navigateTo(url)}>{label}</Button>
</div>
