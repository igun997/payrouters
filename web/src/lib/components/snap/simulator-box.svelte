<script lang="ts">
	/* Sandbox-only outcome buttons. The page renders this only when the API sets allow_simulate. */
	import FlaskConical from '@lucide/svelte/icons/flask-conical';
	import { Button } from '#lib/components/ui/button';
	import type { SimulateOutcome } from '#lib/api/types';

	let { busy, onsimulate }: { busy: boolean; onsimulate: (outcome: SimulateOutcome) => void } =
		$props();
</script>

<section
	class="flex flex-col gap-3 rounded-lg border border-dashed border-border bg-card p-card"
	data-testid="snap-simulator"
	aria-labelledby="simulator-title"
>
	<div class="flex items-center gap-2 text-sandbox-foreground">
		<FlaskConical class="size-4" aria-hidden="true" />
		<h2 id="simulator-title" class="text-micro">Sandbox simulator</h2>
	</div>
	<p class="text-helper text-muted-foreground">Simulate the outcome of this payment</p>
	<div class="grid grid-cols-3 gap-2">
		<Button data-testid="snap-simulate-paid" disabled={busy} onclick={() => onsimulate('paid')}>
			Paid
		</Button>
		<Button
			data-testid="snap-simulate-expired"
			variant="outline"
			disabled={busy}
			onclick={() => onsimulate('expired')}
		>
			Expired
		</Button>
		<Button
			data-testid="snap-simulate-failed"
			variant="destructive-outline"
			disabled={busy}
			onclick={() => onsimulate('failed')}
		>
			Failed
		</Button>
	</div>
</section>
