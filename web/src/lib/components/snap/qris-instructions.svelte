<script lang="ts">
	/* QRIS: the provider image when given, otherwise a code rendered from qr_string. Either can be saved. */
	import QrCode from '#lib/components/app/qr-code.svelte';
	import Banner from '#lib/components/app/banner.svelte';
	import { Button } from '#lib/components/ui/button';
	import Download from '@lucide/svelte/icons/download';
	import type { Instructions, SnapPayment } from '#lib/api/types';

	type Qris = Extract<Instructions, { type: 'qris' }>;

	let { payment, instructions }: { payment: SnapPayment; instructions: Qris } = $props();

	let generated = $state<string | null>(null);
	const src = $derived(instructions.qr_image_url ?? generated);
	const alt = $derived(`QRIS code for order ${payment.order_id}`);
	const fileName = $derived(`qris-${payment.order_id}.png`);
</script>

<section
	class="flex flex-col items-center gap-4 rounded-lg border border-border bg-card p-card"
	data-testid="snap-qris"
>
	<div class="flex w-full flex-col gap-0.5">
		<h2 class="text-card-title font-semibold">QRIS</h2>
		<p class="text-helper text-muted-foreground">Any e-wallet or m-banking app</p>
	</div>

	{#if instructions.qr_image_url}
		<img
			src={instructions.qr_image_url}
			{alt}
			width="240"
			height="240"
			class="h-auto w-[200px] max-w-full rounded-md bg-white p-2 xs:w-[240px]"
		/>
	{:else}
		<QrCode
			bind:src={generated}
			text={instructions.qr_string}
			size={480}
			{alt}
			class="w-[200px] xs:w-[240px]"
		/>
	{/if}

	<Button variant="outline" href={src ?? undefined} download={fileName} disabled={!src}>
		<Download data-icon="inline-start" aria-hidden="true" />
		Save QR image
	</Button>
	<p class="text-helper text-muted-foreground">Open your app, tap Scan, point at the code</p>
</section>

<Banner tone="info">
	The amount is already set in the code. The payment is confirmed automatically once it goes
	through.
</Banner>
