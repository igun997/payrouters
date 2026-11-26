<script lang="ts">
	// Payments as a responsive list (DataList). `full` is the Payments page (created date, chevron);
	// `recent` is the Overview list (time only). Provider and method fold under the product below lg.
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import type { AdminPayment } from '#lib/api/types';
	import DataList, { type DataColumn } from './data-list.svelte';
	import StatusBadge from './status-badge.svelte';
	import * as Table from '#lib/components/ui/table';
	import { clockJakarta, listDateTime } from '#lib/admin-view';
	import { formatIdr } from '#lib/format';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';

	let {
		payments,
		loading = false,
		variant = 'full',
		providerLabels,
		methodLabels,
		empty,
		testid = 'payments-list',
		bare = false
	}: {
		payments: AdminPayment[];
		loading?: boolean;
		variant?: 'full' | 'recent';
		providerLabels: Record<string, string>;
		methodLabels: Record<string, string>;
		empty?: Snippet;
		testid?: string;
		bare?: boolean;
	} = $props();

	const columns = $derived<DataColumn[]>(
		variant === 'full'
			? [
					{ header: 'Created' },
					{ header: 'Order' },
					{ header: 'Product' },
					{ header: 'Provider and method', class: 'hidden lg:table-cell' },
					{ header: 'Amount', align: 'end' },
					{ header: 'Status' },
					{ header: '', class: 'w-10' }
				]
			: [
					{ header: 'Time' },
					{ header: 'Order' },
					{ header: 'Product' },
					{ header: 'Provider and method', class: 'hidden lg:table-cell' },
					{ header: 'Amount', align: 'end' },
					{ header: 'Status' }
				]
	);

	function providerLine(p: AdminPayment): string {
		const provider = providerLabels[p.provider] ?? p.provider;
		const method = p.method ? (methodLabels[p.method] ?? p.method) : null;
		return method ? `${provider} · ${method}` : provider;
	}
</script>

{#snippet cells(p: AdminPayment)}
	{#if variant === 'full'}
		<Table.Cell class="whitespace-nowrap text-muted-foreground">
			{@const [day, time] = listDateTime(p.created_at).split(', ')}
			<span class="block lg:inline">{day}</span><span class="hidden lg:inline">, </span><span
				class="block lg:inline"
			>
				{time}
			</span>
		</Table.Cell>
	{:else}
		<Table.Cell class="tabular whitespace-nowrap text-muted-foreground">
			{clockJakarta(p.created_at)} WIB
		</Table.Cell>
	{/if}
	<Table.Cell>
		<a
			href={resolve('/(admin)/payments/[id]', { id: p.id })}
			class="text-mono-id font-mono font-medium underline-offset-4 hover:underline"
		>
			{p.order_id}
		</a>
	</Table.Cell>
	<Table.Cell class="whitespace-normal">
		<span class="block">{p.product.name}</span>
		<span class="block text-helper text-muted-foreground lg:hidden">{providerLine(p)}</span>
	</Table.Cell>
	<Table.Cell class="hidden whitespace-normal text-muted-foreground lg:table-cell">
		<span>{providerLine(p)}</span>
	</Table.Cell>
	<Table.Cell class="tabular text-right font-semibold whitespace-nowrap">
		{formatIdr(p.amount)}
	</Table.Cell>
	<Table.Cell><StatusBadge status={p.status} /></Table.Cell>
	{#if variant === 'full'}
		<Table.Cell class="w-10 text-muted-foreground">
			<ChevronRight class="size-4" aria-hidden="true" />
		</Table.Cell>
	{/if}
{/snippet}

{#snippet card(p: AdminPayment)}
	<a
		href={resolve('/(admin)/payments/[id]', { id: p.id })}
		class="flex items-center gap-3 px-card py-3.5"
	>
		<div class="flex min-w-0 flex-1 flex-col gap-0.5">
			<span class="truncate text-mono-id font-mono font-medium">{p.order_id}</span>
			<span class="text-helper break-words text-muted-foreground">
				{p.product.name} · {providerLine(p)}
			</span>
			{#if variant === 'recent'}
				<span class="tabular text-helper text-muted-foreground">
					{clockJakarta(p.created_at)} WIB
				</span>
			{/if}
		</div>
		<div class="flex shrink-0 flex-col items-end gap-1.5">
			<span class="tabular text-label font-semibold whitespace-nowrap">{formatIdr(p.amount)}</span>
			<StatusBadge status={p.status} />
		</div>
	</a>
{/snippet}

<DataList
	rows={payments}
	{columns}
	{loading}
	{testid}
	rowTestid="payment-row"
	getKey={(p: AdminPayment) => p.id}
	row={cells}
	{card}
	{empty}
	{bare}
/>
