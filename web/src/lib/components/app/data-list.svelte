<script lang="ts" module>
	export type DataColumn = {
		header: string;
		align?: 'start' | 'end';
		/** Extra classes for the header cell (e.g. `hidden lg:table-cell`). */
		class?: string;
	};
</script>

<script lang="ts" generics="T">
	// Responsive list: a table from md up, a card list below md. One DOM is rendered per mode so
	// test ids stay unique. `row` renders the table cells for a row; `card` renders its phone card.
	// Loading shows skeleton rows; `empty` replaces the body when there are no rows.
	import type { Snippet } from 'svelte';
	import * as Table from '#lib/components/ui/table';
	import { Skeleton } from '#lib/components/ui/skeleton';
	import { IsMobile } from '#lib/hooks/is-mobile.svelte';
	import { cn } from '#lib/utils';

	let {
		rows,
		columns,
		getKey,
		row,
		card,
		empty,
		loading = false,
		testid,
		rowTestid = 'row',
		skeletonRows = 5,
		bare = false
	}: {
		rows: T[];
		columns: DataColumn[];
		getKey: (item: T) => string;
		row: Snippet<[T]>;
		card: Snippet<[T]>;
		empty?: Snippet;
		loading?: boolean;
		testid: string;
		rowTestid?: string;
		skeletonRows?: number;
		/** Drops the outer border and radius when a parent card already frames the list. */
		bare?: boolean;
	} = $props();

	const frame = $derived(
		bare
			? 'overflow-hidden border-t border-border'
			: 'overflow-hidden rounded-lg border border-border bg-card'
	);
	const phone = new IsMobile(768);
	const skeleton = $derived(loading);
	const placeholders = $derived(Array.from({ length: skeletonRows }, (_, i) => i));
</script>

{#if phone.current}
	{#if skeleton}
		<div class={cn('flex flex-col divide-y divide-border', frame)}>
			{#each placeholders as i (i)}
				<div class="flex flex-col gap-2 p-card">
					<Skeleton class="h-4 w-32" />
					<Skeleton class="h-4 w-48" />
				</div>
			{/each}
		</div>
	{:else if rows.length === 0}
		{#if empty}{@render empty()}{/if}
	{:else}
		<ul class={cn('flex flex-col divide-y divide-border', frame)} data-testid={testid}>
			{#each rows as item (getKey(item))}
				<li data-testid={rowTestid}>{@render card(item)}</li>
			{/each}
		</ul>
	{/if}
{:else}
	<div class={frame}>
		<Table.Root data-testid={testid}>
			<Table.Header>
				<Table.Row class="hover:bg-transparent">
					{#each columns as column (column.header)}
						<Table.Head class={cn(column.align === 'end' && 'text-right', column.class)}>
							{column.header}
						</Table.Head>
					{/each}
				</Table.Row>
			</Table.Header>
			<Table.Body>
				{#if skeleton}
					{#each placeholders as i (i)}
						<Table.Row>
							{#each columns as column (column.header)}
								<Table.Cell class={column.class}><Skeleton class="h-4 w-full" /></Table.Cell>
							{/each}
						</Table.Row>
					{/each}
				{:else if rows.length === 0}
					<Table.Row class="hover:bg-transparent">
						<Table.Cell colspan={columns.length} class="p-0">
							{#if empty}{@render empty()}{/if}
						</Table.Cell>
					</Table.Row>
				{:else}
					{#each rows as item (getKey(item))}
						<Table.Row data-testid={rowTestid}>{@render row(item)}</Table.Row>
					{/each}
				{/if}
			</Table.Body>
		</Table.Root>
	</div>
{/if}
