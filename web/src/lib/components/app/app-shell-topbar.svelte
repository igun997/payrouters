<script lang="ts">
	// Admin top bar. Phones: brand tile (hidden at 320 to leave room), page title (wraps, never
	// clips) and the mode chip. Desktop: title with subtitle, the date range (when the page shows
	// it), the page's primary actions and the segmented mode switch.
	import BrandMark from './brand-mark.svelte';
	import ModeToggle from './mode-toggle.svelte';
	import DateRangeFilter from './date-range-filter.svelte';
	import { shell } from '#lib/state/shell.svelte';

	let {
		title,
		subtitle = null,
		showRange = false
	}: { title: string; subtitle?: string | null; showRange?: boolean } = $props();
</script>

<header
	class="flex shrink-0 items-center gap-3 border-b border-border bg-background px-page py-3 md:bg-card md:py-4"
>
	<div class="max-xs:hidden md:hidden">
		<BrandMark tile size={22} />
	</div>
	<div class="flex min-w-0 flex-1 flex-col gap-0.5">
		<p class="text-page-title break-words md:truncate" data-testid="shell-title">{title}</p>
		{#if subtitle}
			<p class="hidden truncate text-body text-muted-foreground md:block">{subtitle}</p>
		{/if}
	</div>
	{#if showRange}
		<div class="hidden md:block">
			<DateRangeFilter />
		</div>
	{/if}
	{#if shell.actions}
		<div class="hidden items-center gap-2 md:flex">
			{@render shell.actions()}
		</div>
	{/if}
	<ModeToggle />
</header>
