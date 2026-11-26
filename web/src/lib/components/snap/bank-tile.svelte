<script lang="ts">
	/* Square badge with a bank short code (BCA, BNI, MDR ...). */
	import { cn } from '#lib/utils';

	let { text, class: className }: { text: string; class?: string } = $props();
</script>

<span
	class={cn(
		'flex size-11 shrink-0 items-center justify-center rounded-md bg-secondary text-label font-semibold text-foreground',
		className
	)}
	aria-hidden="true"
>
	{text}
</span>
