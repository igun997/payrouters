<script lang="ts">
	// Two-letter tile identifying a provider (MT, XD, ID) in lists and breakdowns.
	import { providerMark } from '#lib/admin-view';
	import { cn } from '#lib/utils';

	let { provider, class: className }: { provider: string; class?: string } = $props();
</script>

<span
	class={cn(
		'flex size-9 shrink-0 items-center justify-center rounded-md border border-border bg-background font-mono text-helper font-semibold text-foreground',
		className
	)}
	aria-hidden="true"
>
	{providerMark(provider)}
</span>
