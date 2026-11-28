<script lang="ts">
	/* One standard payment method (QRIS or a bank virtual account) in the choose-how-to-pay list. */
	import QrCode from '@lucide/svelte/icons/qr-code';
	import BankTile from './bank-tile.svelte';
	import PayRow from './pay-row.svelte';
	import { bankBadge, methodShortName } from './method-meta';
	import type { Method, SnapMethod } from '#lib/api/types';

	let {
		method,
		state = 'idle',
		disabled = false,
		onpick
	}: {
		method: SnapMethod;
		state?: 'idle' | 'connecting' | 'error';
		/** Another row is starting; this one cannot be tapped. */
		disabled?: boolean;
		onpick: (code: Method) => void;
	} = $props();

	const isQris = $derived(method.group === 'qris');
	const badge = $derived(isQris ? '' : bankBadge(method.code.replace(/^va_/, '')));
	const sub = $derived.by(() => {
		if (state === 'connecting') return `Connecting to ${methodShortName(method.code)}…`;
		if (state === 'error') return 'Could not start this payment. Try again.';
		return isQris ? 'Any e-wallet or m-banking app' : null;
	});
</script>

<PayRow
	testid={`snap-method-${method.code}`}
	{state}
	{disabled}
	title={method.label}
	{sub}
	onclick={() => onpick(method.code)}
>
	{#snippet leading()}
		{#if isQris}
			<span
				class="flex size-11 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground"
				aria-hidden="true"
			>
				<QrCode class="size-5" />
			</span>
		{:else}
			<BankTile text={badge} code={method.code} />
		{/if}
	{/snippet}
</PayRow>
