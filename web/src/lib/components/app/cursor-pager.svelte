<script lang="ts">
	// Previous / next for cursor-paged lists. `start` and `end` are the 1-based rows on this page;
	// `total` is shown when it is known for the current filter.
	import { Button } from '#lib/components/ui/button';
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import { formatInt } from '#lib/format';
	import { cn } from '#lib/utils';

	let {
		start,
		end,
		total = null,
		hasPrevious,
		hasNext,
		busy = false,
		onprevious,
		onnext,
		testid = 'pager',
		framed = false
	}: {
		start: number;
		end: number;
		total?: number | null;
		hasPrevious: boolean;
		hasNext: boolean;
		busy?: boolean;
		onprevious: () => void;
		onnext: () => void;
		testid?: string;
		/** Footer of a card that already frames the list: a top rule and card padding. */
		framed?: boolean;
	} = $props();
</script>

<div
	class={cn(
		'flex flex-col items-start gap-3 sm:flex-row sm:items-center sm:justify-between',
		framed && 'border-t border-border p-card'
	)}
	data-testid={testid}
>
	<span class="tabular text-helper text-muted-foreground" data-testid="pager-status">
		{#if end === 0}
			No rows
		{:else if total !== null}
			Showing {formatInt(start)}–{formatInt(end)} of {formatInt(total)}
		{:else}
			Showing {formatInt(start)}–{formatInt(end)}
		{/if}
	</span>
	<div class="flex gap-2">
		<Button variant="outline" size="sm" onclick={onprevious} disabled={!hasPrevious || busy}>
			<ChevronLeft data-icon="inline-start" />
			Previous
		</Button>
		<Button variant="outline" size="sm" onclick={onnext} disabled={!hasNext || busy}>
			Next
			<ChevronRight data-icon="inline-end" />
		</Button>
	</div>
</div>
