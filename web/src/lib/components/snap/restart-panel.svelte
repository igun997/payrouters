<script lang="ts">
	/*
	 * Starting a payment over: shown when the payment ran out of time (or its hosted checkout was
	 * never created). The server reopens the same payment for its original window, and the customer
	 * picks how to pay again.
	 */
	import RotateCw from '@lucide/svelte/icons/rotate-cw';
	import { Button } from '#lib/components/ui/button';

	let {
		title,
		detail,
		label = 'Start payment again',
		restarting = false,
		onrestart
	}: {
		title: string;
		detail: string;
		label?: string;
		/** The request is in flight. */
		restarting?: boolean;
		onrestart: () => void;
	} = $props();
</script>

<section
	class="flex flex-col gap-4 rounded-lg border border-border bg-card p-card"
	data-testid="snap-restart-panel"
>
	<div class="flex flex-col gap-1">
		<h2 class="text-card-title font-semibold">{title}</h2>
		<p class="text-body text-muted-foreground">{detail}</p>
	</div>
	<Button
		size="lg"
		class="w-full"
		disabled={restarting}
		onclick={onrestart}
		data-testid="snap-restart"
	>
		<RotateCw data-icon="inline-start" aria-hidden="true" />
		{restarting ? 'Starting…' : label}
	</Button>
</section>
