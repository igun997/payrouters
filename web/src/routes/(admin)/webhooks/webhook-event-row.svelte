<script lang="ts" module>
	/** Desktop columns (md and up) shared by the inbound table header and its rows. */
	export const EVENT_COLUMNS =
		'md:grid-cols-[40px_170px_minmax(0,1fr)_140px_minmax(0,1.2fr)] xl:grid-cols-[40px_170px_minmax(0,1fr)_120px_140px_120px_minmax(0,1.2fr)]';
</script>

<script lang="ts">
	import { resolve } from '$app/paths';
	import type { WebhookEvent } from '#lib/api/types';
	import OutcomePill from '#lib/components/app/outcome-pill.svelte';
	import JsonBlock from '#lib/components/app/json-block.svelte';
	import Pill from '#lib/components/app/pill.svelte';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import ShieldCheck from '@lucide/svelte/icons/shield-check';
	import ShieldX from '@lucide/svelte/icons/shield-x';
	import { formatDateTime } from '#lib/format';
	import { cn } from '#lib/utils';

	/**
	 * One inbound provider webhook. A card below md (outcome and time first, details expand under
	 * it), a table row from md. The expanded detail shows the stored headers and body (already
	 * redacted by the server), one JSON element per line.
	 */
	let {
		event,
		providerName,
		accountName,
		expanded,
		ontoggle
	}: {
		event: WebhookEvent;
		providerName: string;
		accountName: string;
		expanded: boolean;
		ontoggle: () => void;
	} = $props();

	const phone = new IsMobile(768);
</script>

{#snippet detail()}
	<div
		data-testid="webhook-event-detail"
		class="flex flex-col gap-4 border-t pt-4 md:col-span-full md:grid md:grid-cols-2 md:gap-6 md:pb-4"
	>
		{#if phone.current}
			<div class="flex items-center justify-between gap-3">
				<span class="text-label">Signature</span>
				<Pill tone={event.verified ? 'success' : 'failed'}>
					{event.verified ? 'Verified' : 'Not verified'}
				</Pill>
			</div>
		{/if}
		<div class="flex min-w-0 flex-col gap-2">
			<span class="text-label">Headers</span>
			<JsonBlock value={event.headers} />
		</div>
		<div class="flex min-w-0 flex-col gap-2">
			<span class="text-label">Body</span>
			<JsonBlock value={event.body} />
		</div>
	</div>
{/snippet}

{#snippet verifiedMark()}
	{#if event.verified}
		<span class="inline-flex items-center gap-1 text-helper text-muted-foreground">
			<ShieldCheck class="size-4" />Verified
		</span>
	{:else}
		<span class="inline-flex items-center gap-1 text-helper text-failed-foreground">
			<ShieldX class="size-4" />Not verified
		</span>
	{/if}
{/snippet}

{#snippet paymentLink()}
	{#if event.payment_id}
		<a
			href={resolve('/(admin)/payments/[id]', { id: event.payment_id })}
			class="inline-flex min-h-touch w-fit items-center text-mono-id font-mono underline-offset-4 hover:underline"
			data-testid="webhook-payment-link"
		>
			{event.payment_id.slice(0, 8)}…
		</a>
	{:else}
		<span class="text-cell text-muted-foreground">—</span>
	{/if}
{/snippet}

<li
	data-testid="webhook-event-row"
	data-outcome={event.outcome}
	data-verified={event.verified}
	class={cn(
		'flex flex-col gap-3 p-card md:grid md:items-center md:gap-2 md:gap-x-4 md:px-6 md:py-3',
		EVENT_COLUMNS
	)}
>
	{#if phone.current}
		<div class="flex items-center justify-between gap-3">
			<OutcomePill outcome={event.outcome} />
			<span class="tabular shrink-0 text-helper whitespace-nowrap text-muted-foreground">
				{formatDateTime(event.received_at)}
			</span>
		</div>
		<div class="flex items-start justify-between gap-3">
			<div class="flex min-w-0 flex-col gap-1">
				<span class="text-label font-semibold">{providerName}</span>
				<span class="min-w-0 text-helper break-words text-muted-foreground">{accountName}</span>
				{@render paymentLink()}
				{#if event.error}
					<span class="text-helper break-words text-failed-foreground">{event.error}</span>
				{/if}
			</div>
			<button
				type="button"
				onclick={ontoggle}
				aria-expanded={expanded}
				class="inline-flex min-h-touch shrink-0 items-center gap-1.5 rounded-md px-2 text-label text-foreground hover:bg-muted"
			>
				{#if expanded}<ChevronDown class="size-4" />{:else}<ChevronRight class="size-4" />{/if}
				{expanded ? 'Hide details' : 'Show details'}
			</button>
		</div>
		{#if expanded}
			{@render detail()}
		{/if}
	{:else}
		<span class="tabular shrink-0 text-cell whitespace-nowrap text-muted-foreground">
			{formatDateTime(event.received_at)}
		</span>

		<div class="flex min-w-0 flex-col">
			<span class="text-cell font-semibold">{providerName}</span>
			<span class="min-w-0 text-helper break-words text-muted-foreground">{accountName}</span>
		</div>

		<div class="flex flex-col items-start gap-1.5">
			<OutcomePill outcome={event.outcome} />
			<span class="xl:hidden">{@render verifiedMark()}</span>
		</div>

		<span class="hidden xl:block">{@render verifiedMark()}</span>

		<div class="flex min-w-0 flex-col gap-1 xl:contents">
			{@render paymentLink()}
			{#if event.error}
				<span class="text-helper break-words text-failed-foreground xl:self-center">
					{event.error}
				</span>
			{:else}
				<span class="hidden text-cell text-muted-foreground xl:block">—</span>
			{/if}
		</div>

		<button
			type="button"
			onclick={ontoggle}
			aria-expanded={expanded}
			class="inline-flex min-h-touch items-center gap-1.5 self-start rounded-md px-2 text-label text-foreground hover:bg-muted md:order-first md:h-9 md:w-9 md:justify-center md:px-0"
		>
			{#if expanded}<ChevronDown class="size-4" />{:else}<ChevronRight class="size-4" />{/if}
			<span class="md:sr-only">{expanded ? 'Hide details' : 'Show details'}</span>
		</button>

		{#if expanded}
			{@render detail()}
		{/if}
	{/if}
</li>
