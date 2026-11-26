<script lang="ts">
	/*
	 * One payment method. States: idle, connecting (spinner, the others are disabled), error (the
	 * provider refused the start; the row can be tapped again to retry).
	 */
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import CircleAlert from '@lucide/svelte/icons/circle-alert';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import QrCode from '@lucide/svelte/icons/qr-code';
	import BankTile from './bank-tile.svelte';
	import { bankBadge, methodShortName } from './method-meta';
	import type { Method, SnapMethod } from '#lib/api/types';
	import { cn } from '#lib/utils';

	type RowState = 'idle' | 'connecting' | 'error';

	let {
		method,
		state = 'idle',
		disabled = false,
		onpick
	}: {
		method: SnapMethod;
		state?: RowState;
		/** Another method is starting; this row cannot be tapped. */
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

<button
	type="button"
	data-testid={`snap-method-${method.code}`}
	data-state={state}
	aria-busy={state === 'connecting'}
	{disabled}
	onclick={() => onpick(method.code)}
	class={cn(
		'flex min-h-touch w-full items-center gap-3 px-card py-3 text-left transition-colors hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none disabled:cursor-not-allowed sm:py-0',
		disabled && state !== 'connecting' && 'opacity-60'
	)}
>
	{#if isQris}
		<span
			class="flex size-11 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground"
			aria-hidden="true"
		>
			<QrCode class="size-5" />
		</span>
	{:else}
		<BankTile text={badge} />
	{/if}
	<span class="flex min-w-0 flex-1 flex-col gap-0.5">
		<span class="text-card-title font-semibold text-foreground">{method.label}</span>
		{#if sub}
			<span
				class={cn(
					'text-helper',
					state === 'error' ? 'text-failed-foreground' : 'text-muted-foreground'
				)}
			>
				{sub}
			</span>
		{/if}
	</span>
	{#if state === 'connecting'}
		<LoaderCircle
			class="size-5 shrink-0 animate-spin text-muted-foreground motion-reduce:animate-none"
			aria-hidden="true"
		/>
	{:else if state === 'error'}
		<CircleAlert class="size-5 shrink-0 text-failed-foreground" aria-hidden="true" />
	{:else}
		<ChevronRight class="size-5 shrink-0 text-muted-foreground" aria-hidden="true" />
	{/if}
</button>
