<script lang="ts">
	// Settled cash-ins as a responsive list. Provider and method fold under the product below lg.
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import type { CashIn } from '#lib/api/types';
	import DataList, { type DataColumn } from './data-list.svelte';
	import * as Table from '#lib/components/ui/table';
	import { listDateTime } from '#lib/admin-view';
	import { formatIdr, formatInt } from '#lib/format';

	let {
		rows,
		loading = false,
		providerLabels,
		methodLabels,
		empty,
		testid = 'cash-in-list',
		bare = false
	}: {
		rows: CashIn[];
		loading?: boolean;
		providerLabels: Record<string, string>;
		methodLabels: Record<string, string>;
		empty?: Snippet;
		testid?: string;
		bare?: boolean;
	} = $props();

	const columns: DataColumn[] = [
		{ header: 'Paid at' },
		{ header: 'Order' },
		{ header: 'Product' },
		{ header: 'Provider and method', class: 'hidden whitespace-normal lg:table-cell' },
		{ header: 'Gross', align: 'end' },
		{ header: 'Fee', align: 'end' },
		{ header: 'Net', align: 'end' }
	];

	function providerLine(row: CashIn): string {
		const provider = providerLabels[row.provider] ?? row.provider;
		const method = row.method ? (methodLabels[row.method] ?? row.method) : null;
		return method ? `${provider} · ${method}` : provider;
	}
</script>

{#snippet cells(row: CashIn)}
	<Table.Cell class="whitespace-normal text-muted-foreground">
		{listDateTime(row.paid_at)}
	</Table.Cell>
	<Table.Cell>
		<a
			href={resolve('/(admin)/payments/[id]', { id: row.payment_id })}
			class="text-mono-id font-mono font-medium underline-offset-4 hover:underline"
		>
			{row.order_id}
		</a>
	</Table.Cell>
	<Table.Cell class="whitespace-normal">
		<span class="block">{row.product.name}</span>
		<span class="block text-helper text-muted-foreground lg:hidden">{providerLine(row)}</span>
	</Table.Cell>
	<Table.Cell class="hidden whitespace-normal text-muted-foreground lg:table-cell">
		<span>{providerLine(row)}</span>
	</Table.Cell>
	<Table.Cell class="tabular text-right whitespace-nowrap">
		{formatIdr(row.gross_amount)}
	</Table.Cell>
	<Table.Cell class="tabular text-right whitespace-nowrap text-muted-foreground">
		− {formatInt(row.fee_amount)}
	</Table.Cell>
	<Table.Cell class="tabular text-right font-semibold whitespace-nowrap">
		{formatIdr(row.net_amount)}
	</Table.Cell>
{/snippet}

{#snippet card(row: CashIn)}
	<a
		href={resolve('/(admin)/payments/[id]', { id: row.payment_id })}
		class="flex flex-col gap-3 px-card py-3.5"
	>
		<div class="flex items-start justify-between gap-3">
			<div class="flex min-w-0 flex-col gap-0.5">
				<span class="truncate text-mono-id font-mono font-medium">{row.order_id}</span>
				<span class="truncate text-helper text-muted-foreground">
					{row.product.name} · {providerLine(row)}
				</span>
			</div>
			<span class="shrink-0 text-helper whitespace-nowrap text-muted-foreground">
				{listDateTime(row.paid_at)}
			</span>
		</div>
		<dl class="grid grid-cols-3 gap-2 text-helper">
			<div class="flex flex-col gap-0.5">
				<dt class="text-muted-foreground">Gross</dt>
				<dd class="tabular text-label whitespace-nowrap">{formatIdr(row.gross_amount)}</dd>
			</div>
			<div class="flex flex-col gap-0.5">
				<dt class="text-muted-foreground">Fee</dt>
				<dd class="tabular text-label whitespace-nowrap">−{formatIdr(row.fee_amount)}</dd>
			</div>
			<div class="flex flex-col gap-0.5">
				<dt class="text-muted-foreground">Net</dt>
				<dd class="tabular text-label font-semibold whitespace-nowrap">
					{formatIdr(row.net_amount)}
				</dd>
			</div>
		</dl>
		{#if row.provider_ref}
			<span class="truncate font-mono text-helper text-muted-foreground">
				ref {row.provider_ref}
			</span>
		{/if}
	</a>
{/snippet}

<DataList
	{rows}
	{columns}
	{loading}
	{testid}
	rowTestid="cash-in-row"
	getKey={(row: CashIn) => row.id}
	row={cells}
	{card}
	{empty}
	{bare}
/>
