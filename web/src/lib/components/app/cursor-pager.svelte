<script lang="ts">
	// Previous / next for cursor-paged lists. `start` and `end` are the 1-based rows on this page;
	// `total` is shown when it is known for the current filter.
	import { Button } from '#lib/components/ui/button';
	import { formatInt } from '#lib/format';
	import { cn } from '#lib/utils';

	let {
		page,
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
		/** 1-based page number, shown when the page has no rows. */
		page: number;
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

	// A disabled button is a flat grey fill, as in the design, not a faded outline.
	const disabledLook =
		'disabled:border-transparent disabled:bg-secondary disabled:text-muted-foreground disabled:opacity-100';
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
			Page {formatInt(page)} · 0 shown
		{:else if total !== null}
			Showing {formatInt(start)}–{formatInt(end)} of {formatInt(total)}
		{:else}
			Showing {formatInt(start)}–{formatInt(end)}
		{/if}
	</span>
	<div class="flex gap-2">
		<Button
			variant="outline"
			class={disabledLook}
			onclick={onprevious}
			disabled={!hasPrevious || busy}
		>
			Previous
		</Button>
		<Button variant="outline" class={disabledLook} onclick={onnext} disabled={!hasNext || busy}>
			Next
		</Button>
	</div>
</div>
