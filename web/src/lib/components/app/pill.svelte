<script lang="ts">
	import type { Snippet } from 'svelte';
	import { cn } from '#lib/utils';

	/** Small labelled chip for modes, outcomes and delivery states. */
	let {
		tone = 'neutral',
		class: className,
		children
	}: {
		tone?: 'neutral' | 'success' | 'pending' | 'failed' | 'sandbox';
		class?: string;
		children: Snippet;
	} = $props();

	const tones = {
		neutral: 'bg-expired text-expired-foreground',
		success: 'bg-success text-success-foreground',
		pending: 'bg-pending text-pending-foreground',
		failed: 'bg-failed text-failed-foreground',
		sandbox: 'bg-sandbox text-sandbox-foreground'
	};
</script>

<span
	class={cn(
		'inline-flex h-[26px] w-fit shrink-0 items-center rounded-full px-2.5 text-helper font-semibold whitespace-nowrap',
		tones[tone],
		className
	)}
>
	{@render children()}
</span>
